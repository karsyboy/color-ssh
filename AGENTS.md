# AGENTS.md

## Project Overview
- `color-ssh` is a single-crate Rust project that builds the `cossh` CLI (`src/main.rs` + `src/lib.rs`).
- `cossh` wraps SSH and managed RDP launches with a TUI or direct mode, syntax highlighting, inventory loading, and encrypted vault support.
- Optimize for safe remote-session behavior, correct config/inventory parsing, and protection of secrets in logs/runtime data.

## Quick Start Checklist
Run this before opening or updating a PR:

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-targets`
4. `cargo build --release`

## Setup Commands
- Clone and enter repo:
  - `git clone https://github.com/karsyboy/color-ssh.git`
  - `cd color-ssh`
- Build once to download dependencies:
  - `cargo build`
- Confirm CLI wiring:
  - `cargo run -- --help`

Runtime dependencies (outside Cargo):
- `ssh` is required.
- `xfreerdp3` or `xfreerdp` is required only for RDP launches.

## Common Tasks
- Build (dev): `cargo build`
- Build (release): `cargo build --release`
- Run interactive mode: `cargo run --`
- Run SSH directly: `cargo run -- ssh user@example.com`
- Run RDP directly: `cargo run -- rdp desktop01`
- Format: `cargo fmt --all`
- Format check: `cargo fmt --all --check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Test: `cargo test --all-targets`
- Optional benchmark matrix: `scripts/bench/hyperfine_cossh_matrix.sh`

Artifact notes:
- Local release binary is built by `cargo build --release`.
- CI release artifacts/Homebrew publishing are handled by `.github/workflows/release.yml` (`cargo-dist`), not by normal PR checks.

## Code Style & Conventions
- Single source of truth for required quality gates: `.github/workflows/ci.yml`.
- `CONTRIBUTING.md` mirrors the same gate commands; if anything conflicts, follow CI workflow commands.
- Formatting is controlled by `rustfmt.toml` (`max_width = 160`).
- Clippy warnings are treated as errors (`-D warnings`).
- Keep code in existing domain modules under `src/` (`auth`, `config`, `inventory`, `process`, `runtime`, `tui`, etc.).
- When behavior changes, update/add tests under `src/test/**` (the suite is wired through `src/test.rs` and per-module `#[path=...]` test modules).
- Prefer Conventional Commit prefixes (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`) for clean changelog grouping.

## Testing Guidance
Minimum local gate (same as CI):
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

CI/release definitions:
- Verification CI: `.github/workflows/ci.yml`
- Release PR/tag automation: `.github/workflows/release-plz.yml`
- Artifact + Homebrew publishing: `.github/workflows/release.yml`

Useful targeted checks:
- Vault area: `cargo test auth::vault --all-targets`
- Inventory tree loader: `cargo test inventory::tree --all-targets`
- CLI/help surface: `cargo run -- --help`

## Repo Workflow
- Work on a feature branch and open PRs into `main`.
- Run full local verification before pushing.
- `release-plz` handles version/changelog automation after merges to `main`.
- `release-plz.yml` path filter is limited to `Cargo.toml` and `src/**`; docs-only changes do not trigger it.
- Tag-triggered release workflow (`release.yml`) is for maintainers and publishing, not normal feature validation.

## Security Considerations
- Never commit real credentials, vault data, or private inventory host data. Always make sure to use generic examples in test like 10.10.10.10 or 'user'.
- Runtime data lives under `~/.color-ssh/` (config, logs, vault, inventory) and should remain local.
- Prefer safe debug mode (`-d`) for diagnostics.
- Raw debug (`-dd`) may capture terminal content, CLI args, and secrets in `~/.color-ssh/logs/cossh.log`; use only for short troubleshooting sessions.
- Prefer redaction patterns via `settings.remove_secrets` when enabling SSH logging/debug for sensitive environments.
- Keep remote clipboard writes disabled unless required (`interactive_settings.allow_remote_clipboard_write` defaults to `false`).
- CI release workflows rely on repository secrets (`RELEASE_PLZ_TOKEN`, `CARGO_REGISTRY_TOKEN`, `HOMEBREW_TAP_TOKEN`); never hardcode tokens.
- Make sure any code that is added does not compromise the integrity of the vault functions and keeps any passwords used from the vault securely store in memory

## How to Verify Changes
Run these before finalizing work:

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all-targets`
4. `cargo run -- --help`

If your changes touch vault/inventory/logging behavior, also run:
- `cargo test auth::vault --all-targets`
- `cargo test inventory::tree --all-targets`

If your changes touch release automation:
- Verify related workflow/config consistency across:
- `.github/workflows/release-plz.yml`
- `.github/workflows/release.yml`
- `release-plz.toml`
- `dist-workspace.toml`
- `cliff.toml`

## Definition of Done
- Intended behavior is implemented and code compiles.
- Format, clippy, and full test suite pass locally.
- Relevant tests were added/updated for behavior changes.
- Docs/help text were updated when user-facing behavior changed.
- No secrets or local runtime artifacts were introduced into git changes.
- `git status` contains only intentional files.

## Deep References
- Project and user docs: [README.md](README.md)
- Contributor expectations: [CONTRIBUTING.md](CONTRIBUTING.md)
- [User Wiki](https://github.com/karsyboy/color-ssh/wiki)
- [Workflow internals](https://github.com/karsyboy/color-ssh/wiki/Developer)
- [Runtime troubleshooting](https://github.com/karsyboy/color-ssh/wiki/Troubleshooting)
- [Shell completions](shell-completion/README.md)
