//! Deterministic benchmark suites and comparison reports.
//!
//! Every quality number here is the result of a rule-based check on a real model
//! output. ATOM labels these as automated benchmark estimates: passing them is
//! evidence, not proof that two models are equivalent.

pub mod report;
pub mod score;
pub mod store;
pub mod suites;

pub use report::render_markdown;
pub use score::{score_output, Scoring};
pub use store::{
    CaseResult, Comparison, ComparisonStore, ComparisonSummary, ConsistencyResult, Environment, ModelRun,
    RunSettings, SuiteResult,
};
pub use suites::{all_cases, suites, CaseDef};
