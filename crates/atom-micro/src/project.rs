//! ATOM Micro project records and persistence.
//!
//! A Micro project owns its dataset copy, trained weights, exports and firmware
//! builds on disk. The JSON record is the index; artifacts are never inferred.

use crate::architecture::Architecture;
use crate::dataset::DatasetInfo;
use crate::hardware::MemoryPlan;
use crate::model::{EvalReport, QuantReport, TrainReport};
use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchSelection {
    pub spec_id: String,
    pub name: String,
    pub family: String,
    pub context: u32,
    pub hidden: u32,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch: u32,
    pub seed: u64,
    pub windows_per_epoch: u32,
}

impl ArchSelection {
    pub fn from_spec(spec: &Architecture) -> Self {
        ArchSelection {
            spec_id: spec.id.to_string(),
            name: spec.name.to_string(),
            family: spec.family.to_string(),
            context: spec.context,
            hidden: spec.hidden,
            epochs: spec.default_epochs,
            learning_rate: spec.default_lr,
            batch: spec.default_batch,
            seed: 1337,
            windows_per_epoch: spec.windows_per_epoch,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainingSummary {
    pub run_id: String,
    pub job_id: String,
    pub model_path: String,
    pub model_bytes: u64,
    pub started_at: String,
    pub finished_at: String,
    pub report: TrainReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantSummary {
    pub format: String,
    pub path: String,
    pub bytes: u64,
    pub report: QuantReport,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportReport {
    pub dir: String,
    pub created_at: String,
    pub files: Vec<ExportFile>,
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub quant_format: String,
    pub compile_check: Option<String>,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FirmwareReport {
    pub dir: String,
    pub target_id: String,
    pub board: String,
    pub generated_at: String,
    pub built: bool,
    pub build_tool: String,
    pub artifact_path: Option<String>,
    pub log_tail: Vec<String>,
    pub error: Option<String>,
    pub deployed: bool,
    pub deployed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceBenchmark {
    pub id: String,
    pub device: String,
    pub firmware: String,
    pub label: String,
    pub tokens_per_second: f64,
    pub latency_ms: f64,
    pub ram_bytes: u64,
    pub flash_bytes: u64,
    pub notes: String,
    pub measured_at: Option<String>,
    pub imported_at: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistillReport {
    pub teacher_model_id: String,
    pub teacher_label: String,
    pub prompts: usize,
    pub teacher_tokens: u64,
    pub duration_ms: u64,
    pub agreement: f64,
    pub report: TrainReport,
    pub dataset: DatasetInfo,
    pub model_path: String,
    pub created_at: String,
    pub method: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MicroProject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub domain: String,
    pub target_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub dataset: Option<DatasetInfo>,
    pub architecture: Option<ArchSelection>,
    pub training: Option<TrainingSummary>,
    pub evaluation: Option<EvalReport>,
    pub quantization: Option<QuantSummary>,
    pub memory: Option<MemoryPlan>,
    pub export: Option<ExportReport>,
    pub firmware: Option<FirmwareReport>,
    pub distillation: Option<DistillReport>,
    pub benchmarks: Vec<DeviceBenchmark>,
    /// Tabular classification workflow. Absent on projects created before it
    /// existed, so the field is optional and defaults to `None`.
    #[serde(default)]
    pub classification: Option<crate::ClassifierRecord>,
}

impl MicroProject {
    pub fn completed_steps(&self) -> Vec<u32> {
        let mut steps = vec![1];
        if self.target_id != "host" {
            steps.push(2);
        }
        if self.dataset.is_some() {
            steps.push(3);
        }
        if self.architecture.is_some() {
            steps.push(4);
        }
        if self.training.is_some() {
            steps.push(5);
        }
        if self.evaluation.is_some() {
            steps.push(6);
        }
        if self.quantization.is_some() {
            steps.push(7);
        }
        if self.memory.is_some() {
            steps.push(8);
        }
        if self.export.is_some() {
            steps.push(9);
        }
        if self.firmware.is_some() {
            steps.push(10);
        }
        if !self.benchmarks.is_empty() {
            steps.push(11);
        }
        steps
    }

    pub fn next_step(&self) -> u32 {
        let done = self.completed_steps();
        (1..=11).find(|s| !done.contains(s)).unwrap_or(11)
    }

    pub fn summary(&self) -> ProjectSummary {
        ProjectSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            domain: self.domain.clone(),
            target_id: self.target_id.clone(),
            updated_at: self.updated_at.clone(),
            params: self.training.as_ref().map(|t| t.report.params),
            dataset_documents: self.dataset.as_ref().map(|d| d.stats.documents),
            val_perplexity: self.training.as_ref().map(|t| t.report.perplexity),
            quant_format: self.quantization.as_ref().map(|q| q.format.clone()),
            firmware_built: self.firmware.as_ref().map(|f| f.built).unwrap_or(false),
            next_step: self.next_step(),
            completed_steps: self.completed_steps().len(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub target_id: String,
    pub updated_at: String,
    pub params: Option<u64>,
    pub dataset_documents: Option<usize>,
    pub val_perplexity: Option<f64>,
    pub quant_format: Option<String>,
    pub firmware_built: bool,
    pub next_step: u32,
    pub completed_steps: usize,
}

/// Filesystem-backed project store rooted at `<ATOM_HOME>/micro`.
#[derive(Clone, Debug)]
pub struct MicroStore {
    root: PathBuf,
}

impl MicroStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        MicroStore { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    pub fn firmware_dir(&self) -> PathBuf {
        self.root.join("firmware")
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.projects_dir().join(id)
    }

    pub fn record_path(&self, id: &str) -> PathBuf {
        self.projects_dir().join(format!("{id}.json"))
    }

    pub fn model_dir(&self, id: &str) -> PathBuf {
        self.project_dir(id).join("models")
    }

    /// Directory holding the imported classification CSV and its checkpoints.
    pub fn classify_dir(&self, id: &str) -> PathBuf {
        self.project_dir(id).join("classification")
    }

    pub fn classify_csv_path(&self, id: &str) -> PathBuf {
        self.classify_dir(id).join("dataset.csv")
    }

    fn ensure(&self) -> Result<()> {
        for dir in [self.projects_dir(), self.exports_dir(), self.firmware_dir()] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<ProjectSummary> {
        let Ok(entries) = std::fs::read_dir(self.projects_dir()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(project) = serde_json::from_str::<MicroProject>(&raw) {
                    out.push(project.summary());
                }
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub fn get(&self, id: &str) -> Result<MicroProject> {
        let path = self.record_path(id);
        let raw = std::fs::read_to_string(&path)
            .map_err(|_| AtomError::NotFound(format!("micro project {id} does not exist")))?;
        serde_json::from_str(&raw)
            .map_err(|e| AtomError::Parse(format!("project {id} record is invalid: {e}")))
    }

    pub fn save(&self, project: &mut MicroProject) -> Result<()> {
        self.ensure()?;
        project.updated_at = chrono::Local::now().to_rfc3339();
        std::fs::create_dir_all(self.project_dir(&project.id))?;
        let target = self.record_path(&project.id);
        let temp = target.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_string_pretty(project)?)?;
        std::fs::rename(temp, target)?;
        Ok(())
    }

    pub fn create(&self, name: &str, description: &str, domain: &str, target_id: &str) -> Result<MicroProject> {
        let name = name.trim();
        if name.len() < 2 {
            return Err(AtomError::Invalid("project name needs at least 2 characters".into()));
        }
        if name.len() > 80 {
            return Err(AtomError::Invalid("project name is limited to 80 characters".into()));
        }
        if crate::hardware::target(target_id).is_none() {
            return Err(AtomError::Invalid(format!("unknown target hardware '{target_id}'")));
        }
        self.ensure()?;
        let now = chrono::Local::now().to_rfc3339();
        let mut project = MicroProject {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.trim().to_string(),
            domain: domain.trim().to_string(),
            target_id: target_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            dataset: None,
            architecture: None,
            training: None,
            evaluation: None,
            quantization: None,
            memory: None,
            export: None,
            firmware: None,
            distillation: None,
            benchmarks: Vec::new(),
            classification: None,
        };
        self.save(&mut project)?;
        Ok(project)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.record_path(id);
        if !path.exists() {
            return Err(AtomError::NotFound(format!("micro project {id} does not exist")));
        }
        std::fs::remove_file(&path)?;
        let dir = self.project_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn dataset_paths(&self, id: &str) -> (PathBuf, PathBuf) {
        crate::dataset::dataset_files(&self.project_dir(id))
    }

    pub fn read_dataset(&self, id: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let (train, val) = self.dataset_paths(id);
        let train_bytes = std::fs::read(&train).map_err(|_| {
            AtomError::Conflict("this project has no imported dataset yet".into())
        })?;
        let val_bytes = std::fs::read(&val).map_err(|_| {
            AtomError::Conflict("the project's validation split is missing; re-import the dataset".into())
        })?;
        Ok((train_bytes, val_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_get_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = MicroStore::new(dir.path());
        let project = store.create("Smart desk", "desk commands", "home automation", "esp32s3").unwrap();
        assert_eq!(store.list().len(), 1);
        let loaded = store.get(&project.id).unwrap();
        assert_eq!(loaded.name, "Smart desk");
        // Steps 1 (project) and 2 (target hardware) are complete after creation.
        assert_eq!(loaded.next_step(), 3);
        store.delete(&project.id).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn unknown_target_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = MicroStore::new(dir.path());
        assert!(store.create("x", "", "", "not-a-chip").is_err());
    }
}
