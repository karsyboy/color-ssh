//! App orchestration and event loop modules.

mod action;
mod event_loop;
mod lifecycle;

pub use lifecycle::run_session_manager;
