#!/usr/bin/env bash
set -euo pipefail

HOST="${1:-localhost}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
DEFAULT_COSSH_BIN="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"
# Path to color-ssh project
COSSH_BIN="${COSSH_BIN:-${DEFAULT_COSSH_BIN}}"
WARMUP="${WARMUP:-2}"
RUNS="${RUNS:-50}"
OUT_ROOT="${OUT_ROOT:-benchmarks/hyperfine}"
SHOW_OUTPUT="${SHOW_OUTPUT:-0}"
BENCH_CACHE_DIR="${BENCH_CACHE_DIR:-${SCRIPT_DIR}/cache}"
NETWORK_CORPUS_FILE="${NETWORK_CORPUS_FILE:-${BENCH_CACHE_DIR}/network-corpus.txt}"

RFC_IDS=(
  768 791 792 793 1034 1035 2131 2328 4271 8200 8446 9000 9110 9293
)

download_url() {
  local url="$1"
  local out_file="$2"

  if command -v wget >/dev/null 2>&1; then
    wget -q -O "${out_file}" "${url}"
    return 0
  fi

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "${out_file}" "${url}"
    return 0
  fi

  echo "Either wget or curl is required to download benchmark corpus" >&2
  exit 1
}

ensure_network_corpus() {
  local corpus_file="$1"
  local corpus_dir
  corpus_dir="$(dirname "${corpus_file}")"
  mkdir -p "${corpus_dir}"

  if [[ -s "${corpus_file}" ]]; then
    return 0
  fi

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' RETURN

  download_url "https://www.rfc-editor.org/rfc/rfc-index.txt" "${tmp_dir}/rfc-index.txt"
  for rfc in "${RFC_IDS[@]}"; do
    download_url "https://www.rfc-editor.org/rfc/rfc${rfc}.txt" "${tmp_dir}/rfc${rfc}.txt"
  done

  cat "${tmp_dir}"/*.txt > "${corpus_file}"
}

is_local_host() {
  case "$1" in
    localhost | 127.0.0.1 | ::1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

ensure_remote_target_file() {
  local host="$1"
  local local_file="$2"
  local remote_file="$3"
  local remote_dir
  remote_dir="$(dirname "${remote_file}")"

  if ssh "${host}" "test -s '${remote_file}'" >/dev/null 2>&1; then
    return 0
  fi

  ssh "${host}" "mkdir -p '${remote_dir}'"
  cat "${local_file}" | ssh "${host}" "cat > '${remote_file}'"
}

if [[ $# -ge 2 ]]; then
  TARGET_FILE="$2"
  TARGET_SOURCE="provided"
else
  ensure_network_corpus "${NETWORK_CORPUS_FILE}"
  TARGET_FILE="${NETWORK_CORPUS_FILE}"
  TARGET_SOURCE="auto-rfc-corpus"

  if ! is_local_host "${HOST}"; then
    ensure_remote_target_file "${HOST}" "${NETWORK_CORPUS_FILE}" "${TARGET_FILE}"
  fi
fi

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine is required but was not found in PATH" >&2
  exit 1
fi

if [[ ! -d "${COSSH_BIN}" ]]; then
  echo "COSSH_BIN directory does not exist: ${COSSH_BIN}" >&2
  exit 1
fi

# Run cargo build to ensure the cossh binary is up to date
(
  cd "${COSSH_BIN}/"
  cargo build --release
)

TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${OUT_ROOT}/${TIMESTAMP}"
mkdir -p "${OUT_DIR}"

NATIVE_SSH_CMD="ssh ${HOST} \"cat ${TARGET_FILE}\""
COSSH_LINUX_CMD="${COSSH_BIN}/target/release/cossh -l -P linux ${HOST} \"cat ${TARGET_FILE}\""
COSSH_NETWORK_CMD="${COSSH_BIN}/target/release/cossh -l -P network ${HOST} \"cat ${TARGET_FILE}\""
COSSH_DEFAULT_CMD="${COSSH_BIN}/target/release/cossh -l -P default ${HOST} \"cat ${TARGET_FILE}\""

cat <<MATRIX
Running benchmark matrix:
  host:          ${HOST}
  target file:   ${TARGET_FILE}
  target source: ${TARGET_SOURCE}
  corpus dir:    ${BENCH_CACHE_DIR}
  warmup:        ${WARMUP}
  runs:          ${RUNS}
  output dir:    ${OUT_DIR}
MATRIX

hyperfine_args=(
  --warmup "${WARMUP}"
  --runs "${RUNS}"
  --command-name "native-ssh" "${NATIVE_SSH_CMD}"
  --command-name "cossh-linux-log" "${COSSH_LINUX_CMD}"
  --command-name "cossh-network-log" "${COSSH_NETWORK_CMD}"
  --command-name "cossh-default-log" "${COSSH_DEFAULT_CMD}"
  --export-json "${OUT_DIR}/results.json"
  --export-markdown "${OUT_DIR}/results.md"
)

if [[ "${SHOW_OUTPUT}" == "1" ]]; then
  hyperfine_args+=(--show-output)
fi

hyperfine "${hyperfine_args[@]}"

echo "Benchmark results written to:"
echo "  ${OUT_DIR}/results.json"
echo "  ${OUT_DIR}/results.md"
