//! ATOM Micro: dataset preparation, real training, evaluation, quantization,
//! memory planning, export and firmware generation for specialist models.
//!
//! Everything measurable here is measured. Training, quantization loss deltas,
//! teacher agreement and hardware benchmarks all come from executed work; the
//! only estimates are the explicitly labelled hardware memory plans.

pub mod architecture;
pub mod bench;
pub mod dataset;
pub mod export;
pub mod firmware;
pub mod hardware;
pub mod model;
pub mod project;
pub mod tools;

pub use architecture::{architecture, architectures, Architecture};
pub use bench::{parse_report, summarize, BenchmarkSummary};
pub use dataset::{analyze, import, parse_documents, split_documents, DatasetInfo, DatasetStats};
pub use export::{export_bundle, ExportInput};
pub use firmware::generate_project;
pub use hardware::{plan, target, targets, MemoryPlan, Target};
pub use model::{ByteMlp, EvalReport, QuantReport, QuantizedModel, TrainConfig, TrainReport};
pub use project::{
    ArchSelection, DeviceBenchmark, DistillReport, ExportReport, FirmwareReport, MicroProject,
    MicroStore, ProjectSummary, QuantSummary, TrainingSummary,
};
pub use tools::Capabilities;

/// Fraction of held-out teacher positions where the student's greedy next byte
/// matches the teacher's. Deterministic and measured from the corpus.
pub fn teacher_agreement(model: &ByteMlp, corpus: &[u8], max_windows: usize) -> f64 {
    if corpus.len() <= model.context + 1 {
        return 0.0;
    }
    let max_start = corpus.len() - model.context - 1;
    let stride = (max_start / max_windows.max(1)).max(1);
    let mut matches = 0u64;
    let mut total = 0u64;
    let mut start = 0usize;
    while start <= max_start && (total as usize) < max_windows {
        let window = &corpus[start..start + model.context];
        let target = corpus[start + model.context];
        if model.predict_byte(window) == target {
            matches += 1;
        }
        total += 1;
        start += stride;
    }
    if total == 0 {
        0.0
    } else {
        matches as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement_is_bounded() {
        let model = ByteMlp::new(4, 8, 9);
        let value = teacher_agreement(&model, b"the teacher wrote this text about leds", 100);
        assert!((0.0..=1.0).contains(&value));
    }

    #[test]
    fn catalog_and_targets_are_consistent() {
        for arch in architectures() {
            if arch.trainable_here {
                assert!(arch.requires.is_none());
            }
        }
        for t in targets() {
            assert!(!t.name.is_empty());
        }
    }
}
