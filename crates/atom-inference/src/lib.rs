//! llama.cpp backend integration.
//!
//! ATOM talks to llama.cpp only through its stable surfaces: the
//! `llama-server` HTTP API (streaming, cancellation, real timings) and the
//! `llama-quantize` CLI. When a binary is missing, the feature is reported as
//! unavailable rather than simulated.

pub mod backend;
pub mod runtime;

pub use backend::{parse_quantize_formats, probe, BackendStatus, ToolInfo};
pub use runtime::{
    complete_at, stream_chat, ChatEvent, ChatMessage, ChatRequest, CompletionOutput, LoadOptions, Runtime,
    RuntimeInfo, Usage,
};
