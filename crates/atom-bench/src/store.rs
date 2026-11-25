//! Comparison run types and persistence.

use atom_core::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSettings {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: Option<i64>,
    pub context_length: u32,
    pub threads: u32,
    pub batch_size: u32,
    pub kv_cache_type_k: String,
    pub kv_cache_type_v: String,
    pub gpu_layers: u32,
    pub use_mmap: bool,
    pub repeats: u32,
}

impl Default for RunSettings {
    fn default() -> Self {
        RunSettings {
            max_tokens: 128,
            temperature: 0.2,
            top_p: 0.95,
            seed: Some(42),
            context_length: 2048,
            threads: 4,
            batch_size: 512,
            kv_cache_type_k: "f16".into(),
            kv_cache_type_v: "f16".into(),
            gpu_layers: 0,
            use_mmap: true,
            repeats: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub os: String,
    pub arch: String,
    pub cpu_count: usize,
    pub total_memory: u64,
    pub atom_version: String,
    pub backend_version: Option<String>,
    pub started_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseResult {
    pub case_id: String,
    pub suite: String,
    pub title: String,
    pub prompt: String,
    pub output: String,
    pub passed: bool,
    pub detail: String,
    pub latency_ms: u64,
    pub tokens_per_second: f64,
    pub completion_tokens: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuiteResult {
    pub suite: String,
    pub title: String,
    pub passed: usize,
    pub total: usize,
    pub pass_rate: f64,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsistencyResult {
    pub prompt: String,
    pub samples: Vec<String>,
    pub agreement: f64,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRun {
    pub model_id: String,
    pub label: String,
    pub quant: String,
    pub kind: String,
    pub file_size: u64,
    pub load_ms: u64,
    pub peak_rss_bytes: u64,
    pub rss_after_load_bytes: u64,
    pub avg_ttft_ms: f64,
    pub avg_tokens_per_second: f64,
    pub total_generation_ms: u64,
    pub suites: Vec<SuiteResult>,
    pub cases: Vec<CaseResult>,
    pub consistency: Option<ConsistencyResult>,
    /// Mean pass rate across quality suites, 0-100. An automated estimate.
    pub quality_index: f64,
    pub error: Option<String>,
}

impl ModelRun {
    pub fn recompute_summary(&mut self) {
        let quality: Vec<&SuiteResult> = self.suites.iter().filter(|s| s.kind == "quality").collect();
        if quality.is_empty() {
            self.quality_index = 0.0;
        } else {
            self.quality_index = quality.iter().map(|s| s.pass_rate).sum::<f64>() / quality.len() as f64;
        }
        let rated: Vec<&CaseResult> = self.cases.iter().filter(|c| c.tokens_per_second > 0.0).collect();
        self.avg_tokens_per_second = if rated.is_empty() {
            0.0
        } else {
            rated.iter().map(|c| c.tokens_per_second).sum::<f64>() / rated.len() as f64
        };
        self.total_generation_ms = self.cases.iter().map(|c| c.latency_ms).sum();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub settings: RunSettings,
    pub environment: Environment,
    pub runs: Vec<ModelRun>,
    pub job_id: Option<String>,
    pub notes: String,
}

impl Comparison {
    pub fn baseline(&self) -> Option<&ModelRun> {
        self.runs.first()
    }

    pub fn candidate(&self) -> Option<&ModelRun> {
        self.runs.get(1)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComparisonSummary {
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub models: Vec<String>,
    pub baseline_quant: String,
    pub candidate_quant: String,
    pub size_saved_percent: Option<f64>,
    pub speed_change_percent: Option<f64>,
    pub quality_delta: Option<f64>,
    pub job_id: Option<String>,
}

pub struct ComparisonStore {
    dir: PathBuf,
}

impl ComparisonStore {
    pub fn new(dir: PathBuf) -> Self {
        ComparisonStore { dir }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn save(&self, comparison: &Comparison) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.file(&comparison.id).with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(comparison)?)?;
        std::fs::rename(tmp, self.file(&comparison.id))?;
        // Human-readable companion report.
        let md = crate::report::render_markdown(comparison);
        std::fs::write(self.dir.join(format!("{}.md", comparison.id)), md)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Comparison> {
        let raw = std::fs::read_to_string(self.file(id))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let f = self.file(id);
        if f.exists() {
            std::fs::remove_file(&f)?;
        }
        let md = self.dir.join(format!("{id}.md"));
        if md.exists() {
            std::fs::remove_file(md)?;
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<ComparisonSummary> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else { continue };
            let Ok(cmp) = serde_json::from_str::<Comparison>(&raw) else { continue };
            let baseline = cmp.runs.first();
            let candidate = cmp.runs.get(1);
            let size_saved = match (baseline, candidate) {
                (Some(b), Some(c)) if b.file_size > 0 => {
                    Some((b.file_size as f64 - c.file_size as f64) / b.file_size as f64 * 100.0)
                }
                _ => None,
            };
            let speed_delta = match (baseline, candidate) {
                (Some(b), Some(c)) if b.avg_tokens_per_second > 0.0 => {
                    Some((c.avg_tokens_per_second - b.avg_tokens_per_second) / b.avg_tokens_per_second * 100.0)
                }
                _ => None,
            };
            let quality_delta = match (baseline, candidate) {
                (Some(b), Some(c)) => Some(c.quality_index - b.quality_index),
                _ => None,
            };
            out.push(ComparisonSummary {
                id: cmp.id.clone(),
                created_at: cmp.created_at.clone(),
                title: cmp.title.clone(),
                models: cmp.runs.iter().map(|r| r.label.clone()).collect(),
                baseline_quant: baseline.map(|r| r.quant.clone()).unwrap_or_default(),
                candidate_quant: candidate.map(|r| r.quant.clone()).unwrap_or_default(),
                size_saved_percent: size_saved,
                speed_change_percent: speed_delta,
                quality_delta,
                job_id: cmp.job_id.clone(),
            });
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }
}
