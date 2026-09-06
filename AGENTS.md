AGENTS.md

Purpose

This file provides repository-specific operating instructions for AI coding agents working on color-ssh.

Primary goals, in order:

1. Correctness and production safety.
2. Preserve existing user behavior unless intentionally changing it.
3. Make the smallest change that solves the root cause.
4. Verify changes proportionally to their risk.
5. Minimize unnecessary tool calls, command output, context usage, and repeated work.

Higher-level system/user instructions take precedence over this file.

⸻

Project

color-ssh is a single-crate Rust project producing the cossh CLI.

Primary user flows include:

* Interactive TUI session management
* SSH launches
* RDP launches
* Configuration and profiles
* YAML inventory
* SSH config migration
* Vault and askpass authentication
* Session/debug logging
* Terminal and process management

Production code is under src/:

* args
* auth
* config
* inventory
* log
* platform
* process
* runtime
* ssh_config
* terminal
* tui

Entry points:

* src/main.rs
* src/lib.rs

Supported platforms are Linux and macOS. Windows usage is through WSL.

⸻

Source of Truth

Do not assume documentation, comments, tests, or this file are perfectly current.

Use this priority when resolving repository facts:

1. Current production code defines actual runtime behavior.
2. .github/workflows/ci.yml defines required CI verification.
3. Cargo and release configuration define build/package behavior.
4. Tests define expected behavior but may themselves contain defects.
5. Documentation describes intended behavior and should be checked against implementation.

If these disagree, investigate the discrepancy.

Do not change working production behavior solely because documentation or this file claims something different.

Treat AGENTS.md as operational guidance, not proof that the implementation is correct.

⸻

Agent Efficiency Rules

Minimize Context Usage

Use the smallest amount of repository context required to complete the task correctly.

Prefer:

* Search before reading.
* Symbols before whole files.
* Relevant file ranges before complete large files.
* Diffs before rereading modified files.
* Targeted tests before broad tests.
* Existing test helpers before creating new infrastructure.

Avoid:

* Reading the entire repository unless the task explicitly requires a repository-wide review.
* Dumping large source files into context unnecessarily.
* Reading generated files unless directly relevant.
* Reopening files whose relevant contents are already known.
* Repeating searches that already answered the question.
* Running the same verification command repeatedly without an intervening relevant change.
* Printing large successful compiler/test logs.
* Performing speculative exploration after the root cause is established.

Use tools such as rg, targeted file reads, symbol search, and git diff to narrow scope before loading implementation details.

⸻

Command Output Discipline

Successful command output generally has little diagnostic value.

When possible:

* Use quiet/concise output during iteration.
* Record successful checks as PASS rather than retaining their entire output.
* On failure, inspect only the relevant error and surrounding context.
* Do not repeatedly surface identical compiler/test errors.
* Do not request verbose/backtrace output unless the normal failure is insufficient to diagnose the problem.

Increase verbosity only when needed to identify the root cause.

⸻

Verification Cost Strategy

Do not run the full repository verification suite after every edit.

Use staged verification.

Stage 1 — Static reasoning

Before compiling:

1. Locate the relevant implementation.
2. Inspect its callers/callees where behavior crosses boundaries.
3. Inspect existing related tests.
4. Determine the likely root cause.
5. Make the smallest appropriate change.

Do not compile merely to discover information that static inspection can answer cheaply.

Stage 2 — Targeted verification

During implementation, run the narrowest useful check.

Examples:

cargo check --quiet

or a relevant test/module filter:

cargo test --quiet <filter>

Prefer one meaningful targeted test over repeatedly running all tests.

If the targeted check fails, diagnose and fix it before escalating to broader verification.

Stage 3 — Final verification

For completed Rust/code changes intended to be committed or merged, run the required CI gates once after the implementation is stable:

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets

Do not rerun the entire set unless:

* a subsequent code change could affect the result,
* a failure required additional changes,
* or the task explicitly requires repeated validation.

If only documentation or non-executable text changed, do not compile the project unless the change affects executable examples, generated content, packaging, or runtime behavior.

⸻

Task Scope

Choose verification depth based on the task.

Small/local change

Examples:

* isolated bug fix
* small feature
* UI behavior adjustment
* parser correction
* targeted refactor

Workflow:

1. Inspect affected code.
2. Inspect direct callers/callees as needed.
3. Inspect related tests.
4. Implement minimal fix.
5. Run targeted verification.
6. Run final CI gates once when finished.

Do not audit unrelated modules.

⸻

Cross-cutting change

Examples:

* config architecture
* terminal lifecycle
* process management
* logging
* authentication
* shared state
* platform abstraction

Inspect every production path materially affected by the shared behavior.

Do not assume a locally correct change is globally correct.

Pay special attention to:

* initialization
* normal flow
* errors
* cancellation
* shutdown
* cleanup
* persistence
* cross-platform behavior

Use targeted tests while iterating, then final CI gates once.

⸻

Full audit / release readiness

When explicitly asked for a full repository audit, production review, or release-readiness assessment, narrow-scope rules do not limit review breadth.

Inspect all relevant production modules and important cross-module flows.

However, efficiency rules still apply:

* Do not explain healthy code.
* Do not repeatedly reread files.
* Search for related implementations before opening them.
* Consolidate issues with the same root cause.
* Do not run identical tests repeatedly.
* Run broad verification after static inspection rather than using tests as the primary discovery method.
* Report only evidence-backed production issues.

For release readiness, additionally run:

cargo build --release

Run CLI/help validation when CLI behavior, argument parsing, command routing, or help text is relevant.

Inspect release configuration when packaging/release behavior is in scope.

⸻

Engineering Workflow

For bug fixes:

1. Establish the failing user flow.
2. Locate the implementation responsible.
3. Trace enough surrounding behavior to identify the root cause.
4. Confirm the defect is real rather than speculative.
5. Implement the smallest root-cause fix.
6. Add or update regression coverage where practical.
7. Run targeted verification.
8. Inspect the resulting diff.
9. Run required final verification once.
10. Report what changed and what was verified.

Do not fix symptoms when the underlying cause can be addressed safely.

Do not bundle unrelated cleanup or refactoring into a bug fix.

⸻

Editing Rules

Prefer:

* Existing architecture and abstractions.
* Existing naming/style.
* Small focused changes.
* Explicit error handling.
* Behavior-oriented regression tests.
* Existing helpers when appropriate.

Avoid:

* Unnecessary new abstractions.
* Broad refactors for localized problems.
* Duplicate implementations.
* Suppressing warnings rather than correcting them.
* Weakening assertions to make tests pass.
* Removing tests because they expose a bug.
* Converting recoverable runtime failures into panics.
* Adding dependencies when existing code or the standard library reasonably solves the problem.

Before creating a new abstraction, verify an equivalent one does not already exist.

⸻

Critical Production Areas

Changes in these areas require extra scrutiny:

* CLI parsing and command routing
* SSH argument construction
* RDP argument construction
* PTY lifecycle
* Child-process lifecycle
* Signals and shutdown
* Terminal state restoration
* TUI state/input handling
* Config loading and hot reload
* Inventory parsing and persistence
* Migration behavior
* Vault encryption/storage
* Password and askpass handling
* Logging and redaction
* Clipboard handling
* Filesystem permissions
* Platform-specific behavior
* Concurrent/shared state

For these paths, consider both success and failure behavior.

Pay particular attention to bugs that occur through interactions between otherwise-correct modules.

⸻

Runtime Safety

cossh interacts with remote systems, user credentials, terminals, external processes, and persistent user data.

Unless explicitly required by the task:

* Do not initiate real SSH connections.
* Do not initiate real RDP connections.
* Do not launch the interactive TUI unattended.
* Do not run migrations against the user’s real SSH configuration.
* Do not modify real files under ~/.color-ssh/.
* Do not use the user’s real credentials, vault, inventory, hostnames, or private infrastructure.
* Do not enable raw session/debug logging against real sensitive sessions.

For tests and reproductions involving user directories or configuration, use existing test helpers or isolated temporary directories.

Never assume modifying HOME, cwd, environment variables, or global state is safe without restoring it.

⸻

Security Invariants

Never expose or commit:

* passwords
* vault contents
* private keys
* authentication tokens
* askpass values
* sensitive terminal contents
* private inventory data
* repository release tokens

Avoid increasing the lifetime or number of copies of secrets in memory without necessity.

Do not include secrets in:

* logs
* error messages
* debug output
* test fixtures
* snapshots
* CLI diagnostics

Use documentation-safe hostnames and reserved/example addresses in tests and examples.

Security behavior must fail safely.

⸻

Terminal and Process Invariants

Whenever relevant, verify:

* terminal modes are restored after success,
* terminal modes are restored after errors,
* terminal modes are restored after cancellation/exit where controllable,
* child processes are not unintentionally orphaned,
* PTY resources are cleaned up,
* signals are handled consistently,
* resize events do not corrupt state,
* external-command failures reach the user meaningfully.

A remote-session failure should not leave the user’s local terminal unusable.

⸻

Persistence Invariants

For config, inventory, vault, migration, or other persistent state:

* Handle missing files gracefully where appropriate.
* Handle malformed input without uncontrolled panics.
* Avoid unintended truncation or corruption.
* Preserve unrelated user data when modifying structured files.
* Propagate meaningful I/O and permission errors.
* Consider partial-failure behavior.
* Avoid writing outside intended application paths.

⸻

Platform Behavior

Changes to platform-sensitive code must consider both supported native platforms:

* Linux
* macOS

Do not assume Linux-specific:

* commands
* paths
* shell behavior
* PTY behavior
* clipboard mechanisms
* filesystem semantics

Use existing platform abstractions where available rather than scattering new platform checks.

⸻

Tests

Test organization:

* src/test/** — module-attached unit/component tests.
* tests/** — black-box/integration CLI tests.

Follow src/test/README.md for detailed placement conventions.

For bug fixes, prefer a regression test that fails before the fix and passes afterward.

Prefer externally meaningful behavior over implementation-detail assertions.

Avoid duplicate coverage unless it provides meaningful additional confidence.

Do not add large test infrastructure for a defect that can be covered simply.

⸻

Release and CI

.github/workflows/ci.yml is the authoritative CI gate.

Required CI verification:

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets

Relevant release files include:

* .github/workflows/release-plz.yml
* .github/workflows/release.yml
* release-plz.toml
* dist-workspace.toml
* cliff.toml

release.yml is managed/generated by cargo-dist.

Do not manually modify generated cargo-dist workflow logic unless the task specifically requires doing so and regeneration is understood.

Prefer changing the appropriate cargo-dist configuration and regenerating generated artifacts.

Do not trigger publishing/release actions merely to verify a code change.

⸻

Generated and Large Files

Avoid reading generated or very large files in full unless directly required.

When working with:

* generated workflows
* lockfiles
* large fixtures
* generated documentation
* logs

search for the relevant section or inspect the diff first.

Do not spend context explaining generated code unless the task specifically concerns it.

⸻

Git Discipline

Before editing, understand the existing working tree.

Do not overwrite unrelated user changes.

After editing, inspect:

git diff --stat
git diff

or narrower equivalent diffs when the full diff is large.

Ensure only intended files changed.

Do not commit:

* runtime artifacts
* debug logs
* credentials
* temporary files
* local configuration
* unrelated formatting changes

⸻

Stop Conditions

Stop investigating when all of the following are true:

* the relevant behavior is understood,
* the root cause or requested implementation is established,
* additional exploration is unlikely to change the solution,
* the implementation is complete,
* appropriate verification has passed.

Do not continue searching merely to consume additional possibilities.

For audits, continue until the requested review scope has been covered.

Do not invent defects when evidence does not support them.

⸻

Final Response

Keep completion reports concise.

For implementation tasks, report:

1. What changed.
2. Important behavioral impact.
3. Tests/checks performed.
4. Anything not verified.

Do not paste successful test output unless specifically requested.

For bugs or audits, provide actionable evidence:

* location
* trigger
* root cause
* impact
* recommended fix
* regression test

Do not provide lengthy summaries of code that required no changes.

⸻

Definition of Done

A code task is complete when:

* the requested behavior is implemented,
* the root cause is addressed,
* relevant regression coverage exists where practical,
* required verification appropriate to the task has passed,
* user-facing documentation/help is updated when necessary,
* no secrets or runtime artifacts were introduced,
* no unrelated user changes were overwritten,
* the final diff contains only intentional changes.

If verification could not be performed, state that explicitly rather than claiming success.