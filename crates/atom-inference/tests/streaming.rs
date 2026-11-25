//! Integration tests for `llama-server` supervision and SSE token streaming,
//! driven by a stub server process.
//!
//! They prove measured load time, first-token latency, token counting, usage
//! reporting, mid-stream cancellation and HTTP error propagation against a real
//! child process instead of a mocked client.

use atom_inference::{
    complete_at, stream_chat, ChatEvent, ChatMessage, ChatRequest, LoadOptions, Runtime,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::mpsc;

static ENV: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn stub() -> PathBuf {
    atom_teststubs::bin("stub-llama-server")
}

fn model_file(dir: &std::path::Path) -> String {
    let path = dir.join("model.gguf");
    std::fs::write(&path, atom_models::gguf::fixture::tiny_model(atom_models::quant::GGML_TYPE_Q4_K))
        .unwrap();
    path.display().to_string()
}

async fn spawn_stub(model: &str) -> Runtime {
    let options = LoadOptions {
        model_path: model.to_string(),
        model_label: "stub model".into(),
        context_length: 512,
        threads: 2,
        batch_size: 64,
        gpu_layers: 0,
        use_mmap: true,
        use_mlock: false,
        flash_attention: false,
        kv_cache_type_k: "f16".into(),
        kv_cache_type_v: "f16".into(),
        memory_budget_mb: None,
    };
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = logs.clone();
    Runtime::spawn(&stub().display().to_string(), options, move |line| {
        sink.lock().unwrap().push(line);
    })
    .await
    .expect("stub llama-server starts")
}

fn request(max_tokens: u32) -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "What is 2 + 2?".into(),
        }],
        max_tokens,
        temperature: 0.0,
        top_p: 1.0,
        seed: Some(7),
        stop: Vec::new(),
    }
}

async fn collect(rx: &mut mpsc::Receiver<ChatEvent>) -> Vec<ChatEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let done = matches!(event, ChatEvent::Done { .. } | ChatEvent::Error { .. });
        let cancelled = matches!(event, ChatEvent::Cancelled { .. });
        events.push(event);
        if done || cancelled {
            break;
        }
    }
    events
}

#[tokio::test]
async fn load_is_measured_and_the_server_answers_health_and_props() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_LOAD_MS", "300");
    let dir = tempfile::tempdir().unwrap();
    let model = model_file(dir.path());

    let started = std::time::Instant::now();
    let runtime = spawn_stub(&model).await;
    let wall_ms = started.elapsed().as_millis() as u64;

    // The stub sleeps before binding its socket, so the measured load time must
    // include that delay rather than reporting a canned number.
    assert!(
        runtime.info.load_ms >= 250,
        "load_ms {} should reflect the 300 ms stub startup",
        runtime.info.load_ms
    );
    assert!(runtime.info.load_ms <= wall_ms + 50, "load_ms cannot exceed wall time");
    assert_eq!(runtime.info.backend_version.as_deref(), Some("stub-0000"));
    assert!(runtime.info.pid > 0, "a real child pid is recorded");

    let mut runtime = runtime;
    assert!(runtime.is_alive().await);
    runtime.unload().await.expect("child is reaped");
    std::env::remove_var("ATOM_STUB_LOAD_MS");
}

#[tokio::test]
async fn streaming_reports_ttft_tokens_usage_and_throughput() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_TOKENS", "4");
    std::env::set_var("ATOM_STUB_DELAY_MS", "20");
    let dir = tempfile::tempdir().unwrap();
    let model = model_file(dir.path());
    let runtime = spawn_stub(&model).await;

    let (tx, mut rx) = mpsc::channel(64);
    let cancel = Arc::new(AtomicBool::new(false));
    stream_chat(runtime.base_url(), request(32), tx, cancel)
        .await
        .expect("stream completes");
    let events = collect(&mut rx).await;

    let first_ttft = match events.first().expect("stream produced events") {
        ChatEvent::FirstToken { ttft_ms } => {
            assert!(*ttft_ms < 2_000, "ttft should be a real elapsed measurement: {ttft_ms}");
            *ttft_ms
        }
        other => panic!("first event should be FirstToken, got {other:?}"),
    };

    let deltas: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::Delta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas.len(), 4, "one delta per streamed token");
    assert_eq!(deltas.concat(), "The answer is four");

    match events.last().expect("a terminal event") {
        ChatEvent::Done {
            completion_tokens,
            tokens_per_second,
            ttft_ms,
            usage,
            tokens_estimated,
            ..
        } => {
            assert_eq!(*completion_tokens, 4);
            assert!(*tokens_per_second > 0.0, "throughput must be computed from measurement");
            assert_eq!(*ttft_ms, first_ttft, "Done repeats the measured first-token latency");
            assert!(!tokens_estimated, "usage came from the server, so no estimate is used");
            let usage = usage.as_ref().expect("usage block is parsed");
            assert_eq!(usage.completion_tokens, 4);
            assert!(usage.prompt_tokens > 0);
        }
        other => panic!("last event should be Done, got {other:?}"),
    }

    runtime.unload().await.expect("unload");
    std::env::remove_var("ATOM_STUB_TOKENS");
    std::env::remove_var("ATOM_STUB_DELAY_MS");
}

#[tokio::test]
async fn cancellation_mid_stream_reports_partial_token_count() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_TOKENS", "40");
    std::env::set_var("ATOM_STUB_DELAY_MS", "50");
    let dir = tempfile::tempdir().unwrap();
    let model = model_file(dir.path());
    let runtime = spawn_stub(&model).await;

    let (tx, mut rx) = mpsc::channel(64);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let base = runtime.base_url().to_string();
    let worker = tokio::spawn(async move { stream_chat(&base, request(64), tx, flag).await });

    // Wait for real streaming, then cancel like the UI button does.
    let mut seen = 0usize;
    while seen < 2 {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(ChatEvent::Delta { .. })) => seen += 1,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => panic!("stub did not stream tokens"),
        }
    }
    cancel.store(true, Ordering::SeqCst);

    let events = collect(&mut rx).await;
    assert!(
        matches!(events.last(), Some(ChatEvent::Cancelled { tokens }) if *tokens >= 1),
        "expected Cancelled with the tokens actually received, got {:?}",
        events.last()
    );
    tokio::time::timeout(Duration::from_secs(10), worker)
        .await
        .expect("cancelled stream returns promptly")
        .expect("stream task")
        .expect("cancellation is not an error");

    runtime.unload().await.expect("unload");
    std::env::remove_var("ATOM_STUB_TOKENS");
    std::env::remove_var("ATOM_STUB_DELAY_MS");
}

#[tokio::test]
async fn http_error_is_surfaced_not_swallowed() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_HTTP_ERROR", "1");
    let dir = tempfile::tempdir().unwrap();
    let model = model_file(dir.path());
    let runtime = spawn_stub(&model).await;

    let err = complete_at(runtime.base_url(), &request(8))
        .await
        .expect_err("HTTP 500 must fail the request");
    assert!(err.to_string().contains("500"), "{err}");

    let (tx, mut rx) = mpsc::channel(64);
    let result = stream_chat(
        runtime.base_url(),
        request(8),
        tx,
        Arc::new(AtomicBool::new(false)),
    )
    .await;
    assert!(result.is_err(), "streaming must also report the failure");
    let events = collect(&mut rx).await;
    assert!(
        matches!(events.last(), Some(ChatEvent::Error { message }) if message.contains("500")),
        "UI gets an error event, got {:?}",
        events.last()
    );

    runtime.unload().await.expect("unload");
    std::env::remove_var("ATOM_STUB_HTTP_ERROR");
}
