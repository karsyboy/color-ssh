# SSH Throughput Benchmarking

Use this matrix to compare native `ssh` and `cossh` throughput with a consistent command set.

## Prerequisites
- `hyperfine` installed
- SSH access to target host (defaults to `localhost`)
- `wget` or `curl` (used to fetch the default RFC corpus when no target file is provided)

## Run matrix
```bash
scripts/bench/hyperfine_cossh_matrix.sh
```

By default, the script downloads and caches a networking-heavy RFC corpus at:
- `scripts/bench/cache/network-corpus.txt`

Optional host/file override:
```bash
scripts/bench/hyperfine_cossh_matrix.sh localhost /var/log/pacman.log
```

Optional run tuning:
```bash
WARMUP=2 RUNS=100 scripts/bench/hyperfine_cossh_matrix.sh
```

Optional command output streaming (useful for debugging failed commands):
```bash
SHOW_OUTPUT=1 scripts/bench/hyperfine_cossh_matrix.sh
```

## Outputs
Each run writes to `benchmarks/hyperfine/<timestamp>/`:
- `results.json`
- `results.md`

## Commands included
- `native-ssh`: `ssh <host> "cat <target-file>"`
- `cossh-linux-log`: `cossh -l -P linux <host> "cat <target-file>"`
- `cossh-network-log`: `cossh -l -P network <host> "cat <target-file>"`
- `cossh-default-log`: `cossh -l -P default <host> "cat <target-file>"`

## Tracking guidance
For each optimization slice, run once before and once after, then compare:
- mean latency
- standard deviation
- min/max spread
