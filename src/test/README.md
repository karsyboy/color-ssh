# Test Guide

This directory contains the crate-attached test suite for `color-ssh`.

## Testing Model

- `src/test/**`: module-attached unit and component tests.
  - These are linked from production modules with `#[path = "..."] mod tests;`.
  - Use this layer when tests need access to module-private helpers/behavior.
- `tests/**`: black-box integration smoke tests.
  - Use this layer for public CLI behavior (`CARGO_BIN_EXE_cossh`) and externally visible workflows.

## Placement Rules

- Mirror production paths whenever possible.
  - Example: `src/process/rdp_builder.rs` -> `src/test/process/rdp_builder.rs`
  - Example: `src/tui/state/app.rs` -> `src/test/tui/state/app.rs`
- For scenario-heavy modules, keep a single entry file and split behavior into focused submodules.
  - Example: `src/test/tui/features/terminal_session/launch/*.rs`
- If a test does not map to one clear production module, prefer the closest behavior owner over a generic catch-all file.

## Naming Rules

- Use behavior-oriented names: `subject_behavior_expected_result`.
- Prefer table-driven test cases when only input/output variations change.
- Avoid noisy suffixes (`core`, `happy_path`, etc.) unless they add real disambiguation.

## Fixtures and Helpers

- Use local helpers first; lift to `src/test/support/*` only when reused across multiple files.
- Use `src/test/support/state.rs` for global config queue/version resets and scoped `HOME`/cwd environment helpers.
- Keep helper behavior explicit and small; avoid helpers that hide important setup logic.

## Anti-Patterns to Avoid

- Large copy-pasted scenario setup blocks when a table or helper can make intent clearer.
- Over-asserting implementation details that are not part of user-visible behavior.
- Hidden "magic" fixture setup that obscures what the test is validating.
- Duplicating equivalent coverage across multiple modules/layers without a clear confidence gain.
