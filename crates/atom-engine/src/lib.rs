//! Application composition root shared by the HTTP API, the CLI and the Tauri shell.

pub mod chat;
pub mod jobs;
pub mod runtime;
pub mod state;

pub use runtime::{ChatSubmission, LoadOutcome, LoadRequest, MemoryEstimate};
pub use state::{AppState, RuntimeStatus};
