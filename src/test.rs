//! Core unit test index.
//!
//! Tests live under `src/test/` and are attached to production modules via
//! `#[path = "..."] mod tests;` so they keep access to module-private items.
//! The suite is intentionally condensed to core behavior coverage only.
//!
//! Domain layout:
//! - `src/test/auth/*`
//! - `src/test/config/*`
//! - `src/test/inventory/*`
//! - `src/test/log/*`
//! - `src/test/ssh_config/*`
//! - `src/test/tui/*`
//! - `src/test/{args,process,runtime,ssh_args}.rs`

pub(crate) mod support;
