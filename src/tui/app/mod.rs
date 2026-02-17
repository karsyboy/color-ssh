//! App orchestration, event loop, and legacy transitional modules.

mod events;
mod run;

pub use run::run_session_manager;
