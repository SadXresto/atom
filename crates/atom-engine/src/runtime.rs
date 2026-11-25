//! Runtime lifecycle: load a model into llama-server, stream chat, unload.

use atom_core::{AtomError, Result};
use atom_inference::{stream_chat, ChatEvent, ChatMessage, ChatRequest, LoadOptions, Runtime};
use atom_memory::Snapshot;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::chat::{ChatMetric, Conversation};
use crate::state::{AppState, LastInference, RuntimeStatus};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadRequest {
    pub model_id: String,
    pub context_length: Option<u32>,
    pub threads: Option<u32>,
    pub batch_size: Option<u32>,
    pub gpu_layers: Option<u32>,
    pub use_mmap: Option<bool>,
    pub use_mlock: Option<bool>,
    pub flash_attention: Option<bool>,
    pub kv_cache_type_k: Option<String>,
    pub kv_cache_type_v: Option<String>,
    pub memory_budget_mb: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadOutcome {
    pub status: RuntimeStatus,
    pub warnings: Vec<String>,
    pub estimate: MemoryEstimate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEstimate {
    pub weights_bytes: u64,
    pub kv_cache_bytes: Option<u64>,
    pub overhead_bytes: u64,
    pub total_bytes: u64,
    pub system_available_bytes: u64,
    pub formula: String,
}

fn kv_bytes_for(kind: &str) -> f64 {
    match kind {
        "f32" => 4.0,
        "q8_0" => 1.0625,
        "q4_0" | "q4_1" | "iq4_nl" => 0.5625,
        "q5_0" | "q5_1" => 0.6875,
        _ => 2.0,
    }
}

impl AppState {
    pub async fn load_model(&self, req: LoadRequest) -> Result<LoadOutcome> {
        let record = self
            .model(&req.model_id)
            .ok_or_else(|| AtomError::NotFound("model is not in the library".into()))?;
        let backend = self.backend();
        let server_path = backend
            .server
            .as_ref()
            .map(|s| s.path.clone())
            .ok_or_else(|| AtomError::Unavailable(backend.message.clone()))?;
        let cfg = self.config();

        {
            let guard = self.inner.runtime.lock().await;
            if guard.is_some() {
                return Err(AtomError::Conflict(
                    "a model is already loaded; unload it before loading another".into(),
                ));
            }
        }

        let context_length = req.context_length.unwrap_or(cfg.context_length).max(128);
        let threads = req.threads.unwrap_or(cfg.threads).clamp(1, 512);
        let kv_k = req.kv_cache_type_k.clone().unwrap_or(cfg.kv_cache_type_k.clone());
        let kv_v = req.kv_cache_type_v.clone().unwrap_or(cfg.kv_cache_type_v.clone());
        let use_mmap = req.use_mmap.unwrap_or(cfg.use_mmap);
        let budget = req.memory_budget_mb.or(cfg.memory_budget_mb);

        let weights = record.summary.weights_bytes;
        let kv = record
            .summary
            .estimate_kv_bytes(context_length as u64, kv_bytes_for(&kv_k).max(kv_bytes_for(&kv_v)));
        let overhead = 320 * 1024 * 1024; // compute buffers and runtime bookkeeping
        let total = weights + kv.unwrap_or(0) + overhead;
        let system_available = {
            let mut monitor = self.inner.monitor.lock().await;
            monitor.sample(None).available_memory
        };

        let mut warnings = Vec::new();
        if let Some(budget_mb) = budget {
            if total > budget_mb * 1024 * 1024 {
                warnings.push(format!(
                    "Estimated runtime memory ({:.2} GiB) exceeds your {} MiB budget.",
                    total as f64 / 1073741824.0,
                    budget_mb
                ));
            }
        }
        if total > system_available {
            warnings.push(format!(
                "Estimated runtime memory ({:.2} GiB) exceeds currently available system memory ({:.2} GiB). \
                 The OS may swap or the load may fail.",
                total as f64 / 1073741824.0,
                system_available as f64 / 1073741824.0
            ));
        }

        let options = LoadOptions {
            model_path: record.path.clone(),
            model_label: record.label.clone(),
            context_length,
            threads,
            batch_size: req.batch_size.unwrap_or(cfg.batch_size),
            gpu_layers: req.gpu_layers.unwrap_or(cfg.gpu_layers),
            use_mmap,
            use_mlock: req.use_mlock.unwrap_or(cfg.use_mlock),
            flash_attention: req.flash_attention.unwrap_or(cfg.flash_attention),
            kv_cache_type_k: kv_k.clone(),
            kv_cache_type_v: kv_v.clone(),
            memory_budget_mb: budget,
        };
        let options_for_status = options.clone();
        let label = record.label.clone();
        let logbuf = self.inner.logbuf.clone();
        let runtime = Runtime::spawn(&server_path, options, |line| {
            logbuf.push("info", "llama-server", line);
        })
        .await?;
        let status = RuntimeStatus {
            loaded: true,
            model_id: Some(record.id.clone()),
            model_label: Some(record.label.clone()),
            model_path: Some(record.path.clone()),
            pid: Some(runtime.info.pid),
            base_url: Some(runtime.info.base_url.clone()),
            load_ms: Some(runtime.info.load_ms),
            started_at: Some(runtime.info.started_at.clone()),
            backend_version: runtime.info.backend_version.clone(),
            options: Some(options_for_status),
            alive: Some(true),
            rss_bytes: None,
        };
        *self.inner.runtime.lock().await = Some(runtime);
        if let Ok(mut slot) = self.inner.runtime_status.write() {
            *slot = status.clone();
        }
        self.inner.peak_rss.store(0, Ordering::SeqCst);
        self.log("info", "runtime", format!("loaded {label} in {} ms", status.load_ms.unwrap_or(0)));

        Ok(LoadOutcome {
            status,
            warnings,
            estimate: MemoryEstimate {
                weights_bytes: weights,
                kv_cache_bytes: kv,
                overhead_bytes: overhead,
                total_bytes: total,
                system_available_bytes: system_available,
                formula: format!(
                    "weights ({:.2} GiB) + KV cache ({}) + runtime overhead ({:.0} MiB)",
                    weights as f64 / 1073741824.0,
                    kv.map(|k| format!("{:.0} MiB", k as f64 / 1048576.0))
                        .unwrap_or_else(|| "unknown".into()),
                    overhead as f64 / 1048576.0
                ),
            },
        })
    }

    pub async fn unload_model(&self) -> Result<RuntimeStatus> {
        let runtime = self.inner.runtime.lock().await.take();
        match runtime {
            Some(rt) => {
                let message = rt.unload().await?;
                self.log("info", "runtime", message);
            }
            None => return Err(AtomError::Conflict("no model is loaded".into())),
        }
        if let Ok(mut status) = self.inner.runtime_status.write() {
            *status = RuntimeStatus::unloaded();
        }
        Ok(RuntimeStatus::unloaded())
    }

    /// Release runtime resources: stop the model process, clear buffered samples
    /// and the peak counter. This never touches model files.
    pub async fn release_resources(&self) -> Result<String> {
        let was_loaded = self.inner.runtime.lock().await.is_some();
        if was_loaded {
            self.unload_model().await?;
        }
        self.inner.monitor.lock().await.clear_history();
        self.inner.peak_rss.store(0, Ordering::SeqCst);
        if let Ok(mut slot) = self.inner.last_inference.write() {
            *slot = None;
        }
        self.log("info", "runtime", "released runtime resources");
        Ok(if was_loaded {
            "model unloaded, sample buffers and peak counters cleared".into()
        } else {
            "sample buffers and peak counters cleared".to_string()
        })
    }

    pub async fn runtime_status(&self) -> RuntimeStatus {
        let mut status = self
            .inner
            .runtime_status
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| RuntimeStatus::unloaded());
        if status.loaded {
            let alive = {
                let mut guard = self.inner.runtime.lock().await;
                match guard.as_mut() {
                    Some(rt) => rt.is_alive().await,
                    None => false,
                }
            };
            status.alive = Some(alive);
        }
        status
    }

    pub async fn snapshot(&self) -> Snapshot {
        let pid = self.inner.runtime_status.read().ok().and_then(|s| s.pid);
        self.inner.monitor.lock().await.sample(pid)
    }

    /// Buffered samples from the periodic sampler (empty if it is mid-refresh).
    pub fn history(&self) -> Vec<Snapshot> {
        match self.inner.monitor.try_lock() {
            Ok(monitor) => monitor.history(),
            Err(_) => Vec::new(),
        }
    }

    /// Begin a streaming chat. The returned receiver carries real backend events;
    /// the conversation is persisted as soon as the stream ends or is cancelled.
    pub fn submit_chat(&self, req: ChatSubmission) -> Result<mpsc::Receiver<ChatEvent>> {
        let status = self
            .inner
            .runtime_status
            .read()
            .map(|s| s.clone())
            .map_err(|_| AtomError::Internal("runtime status unavailable".into()))?;
        if !status.loaded {
            return Err(AtomError::Conflict("load a model before starting a conversation".into()));
        }
        let base_url = status
            .base_url
            .clone()
            .ok_or_else(|| AtomError::Conflict("run (no server address)".into()))?;
        let model_label = status.model_label.clone().unwrap_or_default();
        let model_id = status.model_id.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut slot) = self.inner.chat_cancel.write() {
            *slot = Some(cancel.clone());
        }

        let conversation_id = match req.conversation_id.clone() {
            Some(id) => id,
            None => {
                let convo = Conversation::new(model_id.clone(), model_label.clone());
                let id = convo.id.clone();
                if let Ok(mut chats) = self.inner.chats.write() {
                    chats.push(convo);
                }
                id
            }
        };
        {
            let mut chats = self
                .inner
                .chats
                .write()
                .map_err(|_| AtomError::Internal("chat store poisoned".into()))?;
            match chats.iter_mut().find(|c| c.id == conversation_id) {
                Some(convo) => {
                    convo.model_label = model_label.clone();
                    convo.model_id = model_id.clone();
                    convo.messages = req.messages.clone();
                    convo.updated_at = chrono::Local::now().to_rfc3339();
                    convo.refresh_title();
                }
                None => {
                    // A client-supplied id that is no longer stored (deleted in
                    // another window): recreate it instead of silently dropping
                    // the turn.
                    let mut convo = Conversation::new(model_id.clone(), model_label.clone());
                    convo.id = conversation_id.clone();
                    convo.messages = req.messages.clone();
                    convo.refresh_title();
                    chats.push(convo);
                }
            }
            crate::state::save_conversations(&self.paths().chat_dir(), &chats);
        }

        let chat_request = ChatRequest {
            messages: req.messages.clone(),
            max_tokens: req.max_tokens.unwrap_or(512),
            temperature: req.temperature.unwrap_or(0.7),
            top_p: req.top_p.unwrap_or(0.95),
            seed: req.seed,
            stop: req.stop.clone().unwrap_or_default(),
        };
        let state = self.clone();
        let (tx, rx) = mpsc::channel::<ChatEvent>(128);

        tokio::spawn(async move {
            let (inner_tx, mut inner_rx) = mpsc::channel::<ChatEvent>(128);
            let id_for_forward = conversation_id.clone();
            let forward = tokio::spawn(async move {
                let mut text = String::new();
                let mut ttft = 0u64;
                let mut done: Option<(u64, f64, u64)> = None;
                let mut cancelled = false;
                while let Some(event) = inner_rx.recv().await {
                    match &event {
                        ChatEvent::Delta { text: delta } => text.push_str(delta),
                        ChatEvent::FirstToken { ttft_ms } => ttft = *ttft_ms,
                        ChatEvent::Done {
                            total_ms,
                            completion_tokens,
                            tokens_per_second,
                            ..
                        } => done = Some((*total_ms, *tokens_per_second, *completion_tokens)),
                        ChatEvent::Cancelled { .. } => cancelled = true,
                        _ => {}
                    }
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
                (id_for_forward, text, ttft, done, cancelled)
            });

            let result = stream_chat(&base_url, chat_request, inner_tx, cancel).await;
            let (conversation_id, text, ttft, done, cancelled) = match forward.await {
                Ok(v) => v,
                Err(_) => (conversation_id, String::new(), 0, None, false),
            };
            if let Err(e) = result {
                state.log("error", "inference", e.to_string());
            }
            let assistant = if text.is_empty() { None } else { Some(text) };
            let (total_ms, tps, tokens) = done.unwrap_or((0, 0.0, 0));
            if let Ok(mut chats) = state.inner.chats.write() {
                if let Some(convo) = chats.iter_mut().find(|c| c.id == conversation_id) {
                    if let Some(content) = assistant {
                        let content = if cancelled && !content.ends_with('…') {
                            format!("{content}…")
                        } else {
                            content
                        };
                        convo.messages.push(ChatMessage {
                            role: "assistant".into(),
                            content,
                        });
                    }
                    if done.is_some() {
                        convo.metrics.push(ChatMetric {
                            ttft_ms: ttft,
                            tokens_per_second: tps,
                            completion_tokens: tokens,
                            total_ms,
                            at: chrono::Local::now().to_rfc3339(),
                        });
                    }
                    convo.updated_at = chrono::Local::now().to_rfc3339();
                }
                crate::state::save_conversations(&state.paths().chat_dir(), &chats);
            }
            if done.is_some() {
                state.record_inference(&model_label, ttft, tps, tokens);
            }
        });
        Ok(rx)
    }

    pub fn cancel_chat(&self) -> bool {
        if let Ok(slot) = self.inner.chat_cancel.read() {
            if let Some(flag) = slot.as_ref() {
                flag.store(true, Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    pub fn record_inference(&self, label: &str, ttft_ms: u64, tps: f64, tokens: u64) {
        self.mark_inference(LastInference {
            model_label: label.to_string(),
            tokens_per_second: tps,
            ttft_ms,
            completion_tokens: tokens,
            finished_at: chrono::Local::now().to_rfc3339(),
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatSubmission {
    pub conversation_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub seed: Option<i64>,
    pub stop: Option<Vec<String>>,
}
