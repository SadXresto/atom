//! Orchestration for the tabular classification workflow in ATOM Micro.
//!
//! Import copies the CSV into the project, train runs as a cancellable job, and
//! export re-validates the checkpoint before reporting success. The source CSV
//! is never modified.

use atom_core::{AtomError, JobKind, Result};
use atom_micro::classify::{
    class_shares, export_classifier, load_model, prepare, profile_csv, save_model,
    train_classifier, ClassifierDataset, ClassifierRecord, ClassifierSpec, ClassifierTraining,
    CsvProfile, TrainSettings,
};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;

#[derive(Clone, Debug, Deserialize)]
pub struct ClassifyProfileRequest {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClassifyImportRequest {
    pub path: String,
    pub target: String,
    pub features: Vec<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub val_ratio: Option<f64>,
    #[serde(default)]
    pub test_ratio: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClassifyTrainRequest {
    #[serde(default)]
    pub epochs: Option<u32>,
    #[serde(default)]
    pub learning_rate: Option<f32>,
    #[serde(default)]
    pub batch: Option<u32>,
    #[serde(default)]
    pub l2: Option<f32>,
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Inspect a CSV without storing anything. Used by the column picker.
pub fn profile_path(state: &AppState, req: ClassifyProfileRequest) -> Result<CsvProfile> {
    let guard = state.guard();
    let path = guard.resolve_existing(&req.path)?;
    Ok(profile_csv(&path)?.profile)
}

/// Copy the CSV into the project, validate the selected columns, and persist
/// the dataset record. Returns the updated project.
pub fn import_dataset(
    state: &AppState,
    project_id: &str,
    req: ClassifyImportRequest,
) -> Result<atom_micro::MicroProject> {
    let mut project = state.inner.micro.get(project_id)?;
    let guard = state.guard();
    let source = guard.resolve_existing(&req.path)?;
    let spec = ClassifierSpec {
        features: req.features.clone(),
        target: req.target.clone(),
        val_ratio: req.val_ratio.unwrap_or(0.15),
        test_ratio: req.test_ratio.unwrap_or(0.15),
        seed: req.seed.unwrap_or(1337),
    };
    // Full validation happens before anything is copied, so an invalid column
    // selection cannot leave a half-configured project behind.
    let (parsed, data) = prepare(&source, &spec)?;
    let dir = state.inner.micro.classify_dir(project_id);
    std::fs::create_dir_all(&dir)?;
    let stored = state.inner.micro.classify_csv_path(project_id);
    std::fs::copy(&source, &stored)?;

    let mut notes = Vec::new();
    if data.design.duplicate_groups > 0 {
        notes.push(format!(
            "{} identical rows were kept in the same split so a duplicate cannot leak across \
             train/validation/test.",
            data.design.duplicate_rows
        ));
    }
    if data.design.missing > 0 {
        notes.push(format!(
            "{} missing numeric values were imputed with the training-split mean and counted in \
             the column profile.",
            data.design.missing
        ));
    }
    if !parsed.profile.warnings.is_empty() {
        notes.extend(parsed.profile.warnings.clone());
    }
    let dataset = ClassifierDataset {
        profile: parsed.profile,
        spec,
        stored_path: stored.display().to_string(),
        design_features: data.design.x.first().map(|r| r.len()).unwrap_or(0),
        classes: class_shares(&data.design.labels, &data.design.y),
        split: atom_micro::classify::SplitCounts {
            train: data.train_index.len(),
            validation: data.val_index.len(),
            test: data.test_index.len(),
        },
        duplicate_rows: data.design.duplicate_rows,
        duplicate_groups: data.design.duplicate_groups,
        missing_values: data.design.missing,
        notes,
    };
    project.classification = Some(ClassifierRecord {
        dataset,
        training: None,
        export: None,
    });
    state.inner.micro.save(&mut project)?;
    state.log(
        "info",
        "micro",
        format!("imported classification dataset for {}", project.name),
    );
    Ok(project)
}

impl AppState {
    /// Start a cancellable classification training job. The stored CSV copy is
    /// re-prepared with the recorded spec so the split is reproducible.
    pub fn start_micro_classify(
        &self,
        project_id: &str,
        req: ClassifyTrainRequest,
    ) -> Result<String> {
        let project = self.inner.micro.get(project_id)?;
        let record = project.classification.clone().ok_or_else(|| {
            AtomError::Conflict("import a classification dataset before training".into())
        })?;
        let settings = TrainSettings {
            epochs: req.epochs.unwrap_or(60).clamp(1, 2_000),
            learning_rate: req.learning_rate.unwrap_or(0.25),
            batch: req.batch.unwrap_or(32).clamp(1, 4096),
            l2: req.l2.unwrap_or(1e-4).max(0.0),
            seed: req.seed.unwrap_or(record.dataset.spec.seed),
        };
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Train,
            format!("Train classifier · {}", project.name),
            json!({
                "project_id": project_id,
                "kind": "classification",
                "target": record.dataset.spec.target,
                "features": record.dataset.spec.features,
                "epochs": settings.epochs,
            }),
        );
        let state = self.clone();
        let project_id = project_id.to_string();
        let spec = record.dataset.spec.clone();
        tokio::spawn(async move {
            job.start();
            job.progress("reading stored CSV copy", 0.01);
            let csv_path = state.inner.micro.classify_csv_path(&project_id);
            let prepared = tokio::task::spawn_blocking(move || prepare(&csv_path, &spec)).await;
            let prepared = match prepared {
                Err(e) => {
                    job.finish_err(format!("classification task failed: {e}"));
                    return;
                }
                Ok(Err(e)) => {
                    job.finish_err(e.to_string());
                    return;
                }
                Ok(Ok(value)) => value,
            };
            let (_, data) = prepared;
            job.log(
                "info",
                format!(
                    "split: {} train / {} validation / {} test rows",
                    data.train_index.len(),
                    data.val_index.len(),
                    data.test_index.len()
                ),
            );
            let job_progress = job.clone();
            let job_log = job.clone();
            let job_cancel = job.clone();
            let started_at = chrono::Local::now().to_rfc3339();
            let result = tokio::task::spawn_blocking(move || {
                let mut last = String::new();
                let outcome = train_classifier(
                    &data,
                    &settings,
                    |stage, fraction| {
                        if stage != last {
                            job_log.log("train", stage);
                            last = stage.to_string();
                        }
                        job_progress.progress(stage, 0.02 + fraction * 0.96);
                    },
                    || job_cancel.is_cancelled(),
                );
                (data, outcome)
            })
            .await;

            let (data, outcome) = match result {
                Err(e) => {
                    job.finish_err(format!("classification task failed: {e}"));
                    return;
                }
                Ok((_, Err(e))) => {
                    if matches!(e, AtomError::Cancelled(_)) {
                        job.log("warn", "no classifier checkpoint was written");
                        job.finish_cancelled();
                    } else {
                        job.finish_err(e.to_string());
                    }
                    return;
                }
                Ok((data, Ok(outcome))) => (data, outcome),
            };

            let model_path = state
                .inner
                .micro
                .classify_dir(&project_id)
                .join(format!("{}.classifier.json", job.id()));
            let bytes = match save_model(&outcome.model, &model_path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    job.finish_err(format!("could not write the classifier: {e}"));
                    return;
                }
            };
            job.log(
                "info",
                format!(
                    "wrote {} ({} bytes) · test accuracy {:.3} · macro F1 {:.3}",
                    model_path.display(),
                    bytes,
                    outcome.report.test.accuracy,
                    outcome.report.test.macro_f1
                ),
            );
            let mut project = match state.inner.micro.get(&project_id) {
                Ok(p) => p,
                Err(e) => {
                    job.finish_err(e.to_string());
                    return;
                }
            };
            let Some(record) = project.classification.as_mut() else {
                job.finish_err("the classification dataset disappeared during training".to_string());
                return;
            };
            record.training = Some(ClassifierTraining {
                job_id: job.id().to_string(),
                model_path: model_path.display().to_string(),
                model_bytes: bytes,
                started_at,
                finished_at: chrono::Local::now().to_rfc3339(),
                report: outcome.report.clone(),
            });
            record.export = None;
            if let Err(e) = state.inner.micro.save(&mut project) {
                job.finish_err(e.to_string());
                return;
            }
            job.finish_ok(json!({
                "project_id": project_id,
                "model_path": model_path.display().to_string(),
                "report": outcome.report,
                "test_rows": data.test_index.len(),
            }));
        });
        Ok(job_id)
    }
}

/// Load the stored checkpoint + CSV, export the bundle, validate it, and
/// persist the export record.
pub fn export_project(
    state: &AppState,
    project_id: &str,
) -> Result<atom_micro::ClassifierExport> {
    let mut project = state.inner.micro.get(project_id)?;
    let record = project
        .classification
        .clone()
        .ok_or_else(|| AtomError::Conflict("import a classification dataset first".into()))?;
    let training = record
        .training
        .clone()
        .ok_or_else(|| AtomError::Conflict("train the classifier before exporting".into()))?;
    let csv_path = state.inner.micro.classify_csv_path(project_id);
    let (_, data) = prepare(&csv_path, &record.dataset.spec)?;
    let model = load_model(std::path::Path::new(&training.model_path))?;
    if model.labels.len() != record.dataset.classes.len() {
        return Err(AtomError::Parse(
            "the checkpoint's class count does not match the dataset record".into(),
        ));
    }
    let dir = state
        .inner
        .micro
        .classify_dir(project_id)
        .join("export");
    let export = export_classifier(&dir, &data, &model, &training.report)?;
    if let Some(record) = project.classification.as_mut() {
        record.export = Some(export.clone());
    }
    state.inner.micro.save(&mut project)?;
    state.log(
        "info",
        "micro",
        format!("exported classifier bundle for {}", project.name),
    );
    Ok(export)
}
