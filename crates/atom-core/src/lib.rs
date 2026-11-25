//! Shared primitives for ATOM: configuration, errors, background jobs and logs.

pub mod config;
pub mod error;
pub mod jobs;
pub mod logbuf;

pub use config::{AtomConfig, Paths};
pub use error::{AtomError, Result};
pub use jobs::{JobHandle, JobKind, JobStatus, JobStore, JobView};
