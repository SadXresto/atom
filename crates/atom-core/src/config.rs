use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{AtomError, Result};

/// On-disk layout. `ATOM_HOME` overrides the default `$HOME/.atom`.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn resolve() -> Paths {
        let root = std::env::var_os("ATOM_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".atom")))
            // Windows does not set HOME for GUI applications.
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".atom")))
            .unwrap_or_else(|| PathBuf::from(".atom"));
        Paths { root }
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Paths {
        Paths { root: root.into() }
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn library_file(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn bench_dir(&self) -> PathBuf {
        self.root.join("benchmarks")
    }

    pub fn chat_dir(&self) -> PathBuf {
        self.root.join("conversations")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn outputs_dir(&self) -> PathBuf {
        self.root.join("compressed")
    }

    pub fn micro_dir(&self) -> PathBuf {
        self.root.join("micro")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AtomConfig {
    /// Directories ATOM may read model files from.
    pub model_roots: Vec<String>,
    /// Directories ATOM may read datasets from (Micro projects).
    pub dataset_roots: Vec<String>,
    pub models_dir: String,
    pub outputs_dir: String,
    pub reports_dir: String,
    pub data_root: String,

    /// Explicit backend binaries. `None` means "search PATH".
    pub llama_quantize_path: Option<String>,
    pub llama_server_path: Option<String>,
    pub llama_cli_path: Option<String>,

    pub threads: u32,
    pub context_length: u32,
    pub gpu_layers: u32,
    pub batch_size: u32,
    pub use_mmap: bool,
    pub use_mlock: bool,
    pub flash_attention: bool,
    pub kv_cache_type_k: String,
    pub kv_cache_type_v: String,
    pub memory_budget_mb: Option<u64>,
    pub verify_checksums: bool,

    /// Network access is off by default; nothing in ATOM downloads unless this is on.
    pub network_enabled: bool,

    pub api_port: u16,

    /// Set once the first-run walkthrough is completed or skipped.
    pub onboarding_completed: bool,

    /// Optional explicit paths for external tools used by the Micro pipeline.
    pub platformio_path: Option<String>,
    pub python_path: Option<String>,
}

impl Default for AtomConfig {
    fn default() -> Self {
        Self::defaults_for(&Paths::resolve())
    }
}

impl AtomConfig {
    pub fn defaults_for(paths: &Paths) -> Self {
        let root = paths.root.display().to_string();
        let mut model_roots = vec![paths.models_dir().display().to_string()];
        let mut dataset_roots = vec![paths.root.join("datasets").display().to_string()];
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for candidate in ["Models", "Downloads", "Documents/models"] {
                model_roots.push(home.join(candidate).display().to_string());
            }
            for candidate in ["datasets", "Documents/datasets", "Downloads"] {
                dataset_roots.push(home.join(candidate).display().to_string());
            }
        }
        AtomConfig {
            model_roots,
            dataset_roots,
            models_dir: paths.models_dir().display().to_string(),
            outputs_dir: paths.outputs_dir().display().to_string(),
            reports_dir: paths.bench_dir().display().to_string(),
            data_root: root,
            llama_quantize_path: None,
            llama_server_path: None,
            llama_cli_path: None,
            threads: default_threads(),
            context_length: 4096,
            gpu_layers: 0,
            batch_size: 512,
            use_mmap: true,
            use_mlock: false,
            flash_attention: true,
            kv_cache_type_k: "f16".into(),
            kv_cache_type_v: "f16".into(),
            memory_budget_mb: None,
            verify_checksums: true,
            network_enabled: false,
            api_port: 8787,
            onboarding_completed: false,
            platformio_path: None,
            python_path: None,
        }
    }

    pub fn load(paths: &Paths) -> Result<Self> {
        let file = paths.config_file();
        if !file.exists() {
            return Ok(Self::defaults_for(paths));
        }
        let raw = std::fs::read_to_string(&file)?;
        let mut cfg: AtomConfig = serde_json::from_str(&raw)?;
        // Older configs may miss paths; keep them anchored to the current root.
        let defaults = Self::defaults_for(paths);
        if cfg.models_dir.is_empty() {
            cfg.models_dir = defaults.models_dir;
        }
        if cfg.outputs_dir.is_empty() {
            cfg.outputs_dir = defaults.outputs_dir;
        }
        if cfg.reports_dir.is_empty() {
            cfg.reports_dir = defaults.reports_dir;
        }
        if cfg.data_root.is_empty() {
            cfg.data_root = defaults.data_root;
        }
        if cfg.model_roots.is_empty() {
            cfg.model_roots = defaults.model_roots;
        }
        if cfg.dataset_roots.is_empty() {
            cfg.dataset_roots = defaults.dataset_roots;
        }
        Ok(cfg)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.root)?;
        std::fs::write(paths.config_file(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.models_dir, &self.outputs_dir, &self.reports_dir] {
            std::fs::create_dir_all(dir)?;
        }
        for root in &self.dataset_roots {
            let path = PathBuf::from(root);
            if path.starts_with(&self.data_root) {
                std::fs::create_dir_all(path)?;
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.threads == 0 || self.threads > 512 {
            return Err(AtomError::Invalid("threads must be between 1 and 512".into()));
        }
        if self.context_length < 128 {
            return Err(AtomError::Invalid("context length must be at least 128".into()));
        }
        for kv in [&self.kv_cache_type_k, &self.kv_cache_type_v] {
            if !matches!(kv.as_str(), "f16" | "f32" | "q8_0" | "q4_0" | "q4_1" | "iq4_nl" | "q5_0" | "q5_1") {
                return Err(AtomError::Invalid(format!("unsupported KV cache type '{kv}'")));
            }
        }
        for root in &self.model_roots {
            if root.trim().is_empty() {
                return Err(AtomError::Invalid("model roots cannot be empty".into()));
            }
        }
        Ok(())
    }
}

fn default_threads() -> u32 {
    std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4).min(32)
}

pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
