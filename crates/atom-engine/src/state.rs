use atom_bench::ComparisonStore;
use atom_compression::ToolStatus;
use atom_core::{logbuf::LogBuffer, AtomConfig, JobStore, Paths, Result};
use atom_inference::{probe, BackendStatus};
use atom_memory::Monitor;
use atom_models::Library;
use atom_security::PathGuard;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::chat::Conversation;

/// Last real inference measurement, shown on the Overview page.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LastInference {
    pub model_label: String,
    pub tokens_per_second: f64,
    pub ttft_ms: u64,
    pub completion_tokens: u64,
    pub finished_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub loaded: bool,
    pub model_id: Option<String>,
    pub model_label: Option<String>,
    pub model_path: Option<String>,
    pub pid: Option<u32>,
    pub base_url: Option<String>,
    pub load_ms: Option<u64>,
    pub started_at: Option<String>,
    pub backend_version: Option<String>,
    pub options: Option<atom_inference::LoadOptions>,
    pub alive: Option<bool>,
    pub rss_bytes: Option<u64>,
}

impl RuntimeStatus {
    pub fn unloaded() -> Self {
        RuntimeStatus {
            loaded: false,
            model_id: None,
            model_label: None,
            model_path: None,
            pid: None,
            base_url: None,
            load_ms: None,
            started_at: None,
            backend_version: None,
            options: None,
            alive: None,
            rss_bytes: None,
        }
    }
}

pub struct Inner {
    pub paths: Paths,
    pub config: RwLock<AtomConfig>,
    pub library: RwLock<Library>,
    pub guard: RwLock<PathGuard>,
    pub jobs: JobStore,
    pub monitor: Arc<tokio::sync::Mutex<Monitor>>,
    pub runtime: tokio::sync::Mutex<Option<atom_inference::Runtime>>,
    pub runtime_status: RwLock<RuntimeStatus>,
    pub comparisons: ComparisonStore,
    pub chats: RwLock<Vec<Conversation>>,
    pub logbuf: Arc<LogBuffer>,
    pub backend: RwLock<BackendStatus>,
    pub tool: RwLock<ToolStatus>,
    pub last_inference: RwLock<Option<LastInference>>,
    pub chat_cancel: RwLock<Option<Arc<std::sync::atomic::AtomicBool>>>,
    pub started_at: chrono::DateTime<chrono::Local>,
    pub peak_rss: AtomicU64,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

impl AppState {
    pub fn new(paths: Paths) -> Result<Self> {
        std::fs::create_dir_all(&paths.root)?;
        let config = AtomConfig::load(&paths)?;
        config.ensure_dirs()?;
        let library = Library::load(paths.library_file())?;
        let (backend, tool) = {
            let b = probe(
                config.llama_server_path.as_deref(),
                config.llama_quantize_path.as_deref(),
                config.llama_cli_path.as_deref(),
            );
            let t = atom_compression::detect_tool(config.llama_quantize_path.as_deref());
            (b, t)
        };
        let guard = build_guard(&config);
        let chats = load_conversations(&paths.chat_dir());
        let state = AppState {
            inner: Arc::new(Inner {
                paths: paths.clone(),
                config: RwLock::new(config),
                library: RwLock::new(library),
                guard: RwLock::new(guard),
                jobs: JobStore::new(),
                monitor: Arc::new(tokio::sync::Mutex::new(Monitor::new(240))),
                runtime: tokio::sync::Mutex::new(None),
                runtime_status: RwLock::new(RuntimeStatus::unloaded()),
                comparisons: ComparisonStore::new(paths.bench_dir()),
                chats: RwLock::new(chats),
                logbuf: Arc::new(LogBuffer::new(2000)),
                backend: RwLock::new(backend),
                tool: RwLock::new(tool),
                last_inference: RwLock::new(None),
                chat_cancel: RwLock::new(None),
                started_at: chrono::Local::now(),
                peak_rss: AtomicU64::new(0),
            }),
        };
        state.inner.logbuf.push("info", "atom", "engine initialised");
        Ok(state)
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    pub fn config(&self) -> AtomConfig {
        self.inner.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    pub fn backend(&self) -> BackendStatus {
        self.inner.backend.read().map(|b| b.clone()).unwrap()
    }

    pub fn tool(&self) -> ToolStatus {
        self.inner.tool.read().map(|t| t.clone()).unwrap()
    }

    /// Re-probe binaries after the user changed paths.
    pub fn refresh_backend(&self) {
        let cfg = self.config();
        let backend = probe(
            cfg.llama_server_path.as_deref(),
            cfg.llama_quantize_path.as_deref(),
            cfg.llama_cli_path.as_deref(),
        );
        let tool = atom_compression::detect_tool(cfg.llama_quantize_path.as_deref());
        if let Ok(mut b) = self.inner.backend.write() {
            *b = backend;
        }
        if let Ok(mut t) = self.inner.tool.write() {
            *t = tool;
        }
        self.inner.logbuf.push("info", "atom", "backend toolchain re-probed");
    }

    pub fn update_config(&self, next: AtomConfig) -> Result<AtomConfig> {
        next.validate()?;
        next.ensure_dirs()?;
        next.save(&self.inner.paths)?;
        let guard = build_guard(&next);
        if let Ok(mut cfg) = self.inner.config.write() {
            *cfg = next.clone();
        }
        if let Ok(mut g) = self.inner.guard.write() {
            *g = guard;
        }
        self.refresh_backend();
        self.inner.logbuf.push("info", "settings", "configuration updated");
        Ok(next)
    }

    pub fn guard(&self) -> PathGuard {
        self.inner.guard.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn models(&self) -> Vec<atom_models::ModelRecord> {
        self.inner
            .library
            .read()
            .map(|lib| lib.list().to_vec())
            .unwrap_or_default()
    }

    pub fn model(&self, id: &str) -> Option<atom_models::ModelRecord> {
        self.inner.library.read().ok().and_then(|lib| lib.get(id).cloned())
    }

    pub fn library_variants(&self, id: &str) -> Vec<atom_models::ModelRecord> {
        self.inner
            .library
            .read()
            .map(|lib| lib.variants_of(id).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn record_peak(&self, rss: u64) {
        self.inner.peak_rss.fetch_max(rss, Ordering::SeqCst);
    }

    pub fn take_peak(&self) -> u64 {
        self.inner.peak_rss.swap(0, Ordering::SeqCst)
    }

    pub fn mark_inference(&self, stats: LastInference) {
        if let Ok(mut slot) = self.inner.last_inference.write() {
            *slot = Some(stats);
        }
    }

    pub fn last_inference(&self) -> Option<LastInference> {
        self.inner.last_inference.read().ok().and_then(|s| s.clone())
    }

    pub fn log(&self, level: &str, target: &str, message: impl Into<String>) {
        self.inner.logbuf.push(level, target, message);
    }

    /// Start the periodic memory sampler used by the System Monitor.
    pub fn spawn_sampler(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            let self_ = state;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(1500));
            loop {
                interval.tick().await;
                let pid = self_
                    .inner
                    .runtime_status
                    .read()
                    .ok()
                    .and_then(|s| s.pid);
                let mut monitor = self_.inner.monitor.lock().await;
                let snap = monitor.sample(pid);
                if let Some(rss) = snap.model_process_rss {
                    self_.inner.peak_rss.fetch_max(rss, Ordering::SeqCst);
                }
                if let Ok(mut status) = self_.inner.runtime_status.write() {
                    if status.loaded {
                        status.rss_bytes = snap.model_process_rss;
                    }
                }
            }
        });
    }
}

pub fn build_guard(config: &AtomConfig) -> PathGuard {
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.push(PathBuf::from(&config.models_dir));
    roots.push(PathBuf::from(&config.outputs_dir));
    roots.push(PathBuf::from(&config.reports_dir));
    for root in &config.model_roots {
        roots.push(PathBuf::from(root));
    }
    PathGuard::new(roots)
}

pub fn load_conversations(dir: &std::path::Path) -> Vec<Conversation> {
    let file = dir.join("conversations.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_conversations(dir: &std::path::Path, chats: &[Conversation]) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(raw) = serde_json::to_string_pretty(chats) {
        let _ = std::fs::write(dir.join("conversations.json"), raw);
    }
}
