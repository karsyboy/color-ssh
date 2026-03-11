//! Core unit test index.
//!
//! Tests live under `src/test/` and are attached to production modules via
//! `#[path = "..."] mod tests;` so they keep access to module-private items.
//!
//! Layout convention:
//! - Mirror production modules under `src/test/**` whenever possible.
//! - Keep scenario-heavy tests grouped by behavior in submodules.
//! - Use `src/test/support/**` for reusable fixtures and global-state guards.
//! - See `src/test/README.md` for contributor guidance and placement rules.
//!
//! Example mappings:
//! - `src/process/rdp_builder.rs` -> `src/test/process/rdp_builder.rs`
//! - `src/tui/state/app.rs` -> `src/test/tui/state/app.rs`
//! - `src/tui/features/terminal_session/launch.rs` -> `src/test/tui/features/terminal_session/launch.rs`

pub(crate) mod support;
