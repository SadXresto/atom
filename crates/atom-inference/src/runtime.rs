//! llama-server process supervision and streaming chat.

use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadOptions {
    pub model_path: String,
    pub model_label: String,
    pub context_length: u32,
    pub threads: u32,
    pub batch_size: u32,
    pub gpu_layers: u32,
    pub use_mmap: bool,
    pub use_mlock: bool,
    pub flash_attention: bool,
    pub kv_cache_type_k: String,
    pub kv_cache_type_v: String,
    pub memory_budget_mb: Option<u64>,
}

impl LoadOptions {
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec![
            "-m".into(),
            self.model_path.clone(),
            "-c".into(),
            self.context_length.to_string(),
            "-t".into(),
            self.threads.to_string(),
            "-b".into(),
            self.batch_size.to_string(),
            "-ngl".into(),
            self.gpu_layers.to_string(),
            "--host".into(),
            "127.0.0.1".into(),
            "-np".into(),
            "1".into(),
        ];
        if self.use_mmap {
            args.push("--mmap".into());
        } else {
            args.push("--no-mmap".into());
        }
        if self.use_mlock {
            args.push("--mlock".into());
        }
        if self.flash_attention {
            args.push("-fa".into());
        }
        if self.kv_cache_type_k != "f16" {
            args.push("--cache-type-k".into());
            args.push(self.kv_cache_type_k.clone());
        }
        if self.kv_cache_type_v != "f16" {
            args.push("--cache-type-v".into());
            args.push(self.kv_cache_type_v.clone());
        }
        args
    }

    /// Human-readable summary of the runtime tuning ATOM is applying.
    pub fn tuning_summary(&self) -> Vec<String> {
        let mut out = vec![
            format!("context {} tokens", self.context_length),
            format!("{} threads", self.threads),
            format!("batch {}", self.batch_size),
        ];
        out.push(if self.use_mmap {
            "mmap on (weights paged from disk)".into()
        } else {
            "mmap off (weights read into anonymous memory)".into()
        });
        if self.use_mlock {
            out.push("mlock on (locks weights in RAM)".into());
        }
        if self.flash_attention {
            out.push("flash attention requested".into());
        }
        out.push(format!("KV cache {}/{}", self.kv_cache_type_k, self.kv_cache_type_v));
        out.push(format!("{} GPU layers", self.gpu_layers));
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub pid: u32,
    pub port: u16,
    pub base_url: String,
    pub model_label: String,
    pub model_path: String,
    pub started_at: String,
    pub load_ms: u64,
    pub options: LoadOptions,
    pub backend_version: Option<String>,
}

pub struct Runtime {
    child: tokio::process::Child,
    pub info: RuntimeInfo,
}

/// Reserve an ephemeral loopback port for llama-server.
async fn free_port() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| AtomError::Backend(format!("could not reserve a local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AtomError::Backend(e.to_string()))?
        .port();
    drop(listener);
    Ok(port)
}

impl Runtime {
    /// Spawn llama-server and wait until it reports healthy. Load time is
    /// measured, never estimated.
    pub async fn spawn(
        server_path: &str,
        options: LoadOptions,
        mut on_log: impl FnMut(String),
    ) -> Result<Runtime> {
        if !PathBuf::from(&options.model_path).is_file() {
            return Err(AtomError::Invalid(format!(
                "model file {} no longer exists",
                options.model_path
            )));
        }
        let port = free_port().await?;
        let args = options.argv();
        let mut cmd = tokio::process::Command::new(server_path);
        cmd.args(&args)
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        let started = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| AtomError::Backend(format!("could not start llama-server: {e}")))?;
        let pid = child.id().unwrap_or(0);

        // Drain both pipes so the process never blocks on a full buffer.
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "llama-server", "{line}");
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "llama-server", "{line}");
                }
            });
        }

        let base_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(AtomError::Backend(format!(
                    "llama-server exited during startup with {status}. Check the log for the reason \
                     (unsupported context size, insufficient memory, or an invalid model)."
                )));
            }
            if Instant::now() > deadline {
                let _ = child.kill().await;
                return Err(AtomError::Backend(
                    "llama-server did not become healthy within 180s; the load was aborted".into(),
                ));
            }
            match client
                .get(format!("{base_url}/health"))
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => break,
                _ => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
        let load_ms = started.elapsed().as_millis() as u64;
        on_log(format!("llama-server ready on port {port} after {load_ms} ms"));
        let version = match client
            .get(format!("{base_url}/props"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) => resp.json::<serde_json::Value>().await.ok().map(|v| {
                v.get("build_info")
                    .and_then(|b| b.as_str())
                    .unwrap_or("unknown")
                    .to_string()
            }),
            Err(_) => None,
        };

        Ok(Runtime {
            child,
            info: RuntimeInfo {
                pid,
                port,
                base_url,
                model_label: options.model_label.clone(),
                model_path: options.model_path.clone(),
                started_at: chrono::Local::now().to_rfc3339(),
                load_ms,
                options,
                backend_version: version,
            },
        })
    }

    pub fn base_url(&self) -> &str {
        &self.info.base_url
    }

    pub async fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Stop the server and reap the child so the OS reclaims its memory.
    pub async fn unload(mut self) -> Result<String> {
        let _ = self.child.kill().await;
        match self.child.wait().await {
            Ok(status) => Ok(format!("llama-server stopped ({status})")),
            Err(e) => Err(AtomError::Backend(format!("failed to reap llama-server: {e}"))),
        }
    }

    pub async fn chat(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<ChatEvent>,
        cancel: Arc<AtomicBool>,
    ) -> Result<()> {
        stream_chat(&self.info.base_url, request, tx, cancel).await
    }

    /// Non-streaming completion used by the benchmark runner. Real timings come
    /// from llama-server's own `timings` block plus wall-clock measurement.
    pub async fn complete(&self, request: &ChatRequest) -> Result<CompletionOutput> {
        complete_at(&self.info.base_url, request).await
    }
}

/// Stream a chat completion against any llama-server base URL.
pub async fn stream_chat(
    base_url: &str,
    request: ChatRequest,
    tx: mpsc::Sender<ChatEvent>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let body = request.to_json(true);
    let client = reqwest::Client::new();
    let started = Instant::now();
    let resp = client
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .map_err(|e| AtomError::Backend(format!("inference request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let _ = tx
            .send(ChatEvent::Error {
                message: format!("llama-server returned {status}: {text}"),
            })
            .await;
        return Err(AtomError::Backend(format!("inference failed with {status}")));
    }

    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut first_token_at: Option<Instant> = None;
    let mut tokens: usize = 0;
    let mut usage: Option<Usage> = None;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            let _ = tx.send(ChatEvent::Cancelled { tokens }).await;
            return Ok(());
        }
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(ChatEvent::Error {
                        message: format!("stream interrupted: {e}"),
                    })
                    .await;
                return Ok(());
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buffer.find('\n') {
            let line: String = buffer.drain(..=idx).collect();
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else { continue };
            if let Some(delta) = value
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                if !delta.is_empty() {
                    if first_token_at.is_none() {
                        first_token_at = Some(Instant::now());
                        let ttft = started.elapsed().as_millis() as u64;
                        let _ = tx.send(ChatEvent::FirstToken { ttft_ms: ttft }).await;
                    }
                    tokens += 1;
                    let _ = tx.send(ChatEvent::Delta { text: delta.to_string() }).await;
                }
            }
            if let Some(u) = value.get("usage").filter(|u| !u.is_null()) {
                usage = Some(Usage {
                    prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    completion_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                });
            }
        }
    }

    let total_ms = started.elapsed().as_millis() as u64;
    let ttft_ms = first_token_at
        .map(|t| t.duration_since(started).as_millis() as u64)
        .unwrap_or(total_ms);
    let completion_tokens = usage
        .as_ref()
        .map(|u| u.completion_tokens)
        .filter(|n| *n > 0)
        .unwrap_or(tokens as u64);
    let tokens_estimated = usage.is_none();
    let gen_ms = total_ms.saturating_sub(ttft_ms).max(1);
    let tps = if completion_tokens > 1 {
        (completion_tokens.saturating_sub(1)) as f64 / (gen_ms as f64 / 1000.0)
    } else {
        0.0
    };
    let _ = tx
        .send(ChatEvent::Done {
            total_ms,
            ttft_ms,
            completion_tokens,
            tokens_per_second: tps,
            usage,
            tokens_estimated,
        })
        .await;
    Ok(())
}

pub async fn complete_at(base_url: &str, request: &ChatRequest) -> Result<CompletionOutput> {
    let body = request.to_json(false);
    let client = reqwest::Client::new();
    let started = Instant::now();
    let resp = client
        .post(format!("{base_url}/v1/chat/completions"))
        .timeout(Duration::from_secs(600))
        .json(&body)
        .send()
        .await
        .map_err(|e| AtomError::Backend(format!("inference request failed: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AtomError::Backend(format!("llama-server returned {status}: {text}")));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AtomError::Backend(format!("unreadable response: {e}")))?;
    let content = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let usage = value.get("usage");
    let prompt_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let server_tps = value
        .get("timings")
        .and_then(|t| t.get("predicted_per_second"))
        .and_then(|v| v.as_f64());
    let server_ttft_ms = value
        .get("timings")
        .and_then(|t| t.get("prompt_ms"))
        .and_then(|v| v.as_f64());
    let total_ms = started.elapsed().as_millis() as u64;
    Ok(CompletionOutput {
        content,
        prompt_tokens,
        completion_tokens,
        total_ms,
        tokens_per_second: server_tps.unwrap_or_else(|| {
            if completion_tokens > 0 {
                completion_tokens as f64 / (total_ms as f64 / 1000.0)
            } else {
                0.0
            }
        }),
        ttft_ms: server_ttft_ms.unwrap_or(0.0),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionOutput {
    pub content: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_ms: u64,
    pub tokens_per_second: f64,
    pub ttft_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: Option<i64>,
    pub stop: Vec<String>,
}

impl ChatRequest {
    fn to_json(&self, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
            "messages": self.messages,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "top_p": self.top_p,
            "stream": stream,
        });
        if let Some(seed) = self.seed {
            body["seed"] = serde_json::json!(seed);
        }
        if !self.stop.is_empty() {
            body["stop"] = serde_json::json!(self.stop);
        }
        if stream {
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        body
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    FirstToken {
        ttft_ms: u64,
    },
    Delta {
        text: String,
    },
    Done {
        total_ms: u64,
        ttft_ms: u64,
        completion_tokens: u64,
        tokens_per_second: f64,
        usage: Option<Usage>,
        tokens_estimated: bool,
    },
    Cancelled {
        tokens: usize,
    },
    Error {
        message: String,
    },
}
