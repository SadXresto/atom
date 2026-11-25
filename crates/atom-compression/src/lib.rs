//! Permanent model compression via the installed `llama-quantize`.
//!
//! Guarantees enforced here:
//! * the source file is opened read-only and never written;
//! * output always goes to a distinct path in the ATOM workspace;
//! * progress is derived from the tool's own tensor counter;
//! * a cancelled or failed run deletes its partial output;
//! * the output is re-parsed and validated before it can enter the library.

pub mod plan;
pub mod run;
pub mod validate;

pub use plan::{
    build_plan, default_output_name, detect_tool, formats_for, FormatOption, QuantizePlan, ToolStatus,
};
pub use run::{run_quantize, QuantizeOutcome};
pub use validate::{validate_output, ValidationCheck, ValidationReport};
