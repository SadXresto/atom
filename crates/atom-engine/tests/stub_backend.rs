//! End-to-end engine tests against stub `llama-quantize` / `llama-server`
//! executables.
//!
//! Unlike the unit tests, these go through the real job orchestration: config
//! probing, the child process, progress, validation, library insertion, runtime
//! load, chat streaming, cancellation and unload. No llama.cpp build, no model
//! download, and nothing simulated in ATOM itself.

use atom_engine::jobs::{ImportRequest, QuantizeRequest};
use atom_engine::{AppState, ChatSubmission, LoadRequest};
use atom_models::gguf::fixture::tiny_model;
use atom_models::quant::{GGML_TYPE_F16, GGML_TYPE_Q4_K};
use atom_models::ModelKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

static ENV: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Engine configured to use the stub binaries, with a writable `incoming` root.
fn state_with_stubs(dir: &Path) -> (AppState, PathBuf) {
    let incoming = dir.join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let paths = atom_core::Paths::from_root(dir.to_path_buf());
    let mut config = atom_core::AtomConfig::load(&paths).unwrap();
    config.model_roots.push(incoming.display().to_string());
    config.llama_quantize_path = Some(atom_teststubs::bin("stub-llama-quantize").display().to_string());
    config.llama_server_path = Some(atom_teststubs::bin("stub-llama-server").display().to_string());
    config.save(&paths).unwrap();
    (AppState::new(paths).expect("engine starts"), incoming)
}

async fn wait(state: &AppState, job_id: &str) -> atom_core::JobView {
    for _ in 0..400 {
        if let Some(job) = state.inner.jobs.get(job_id) {
            if job.status.is_terminal() {
                return job;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("job {job_id} did not finish in time");
}

/// Import a tiny F16 GGUF, padded so a valid quantized output is genuinely
/// smaller than its source.
async fn import_padded_model(state: &AppState, incoming: &Path) -> atom_models::ModelRecord {
    let source = incoming.join("base.gguf");
    let mut bytes = tiny_model(GGML_TYPE_F16);
    bytes.extend(std::iter::repeat_n(0u8, 200_000));
    std::fs::write(&source, bytes).unwrap();
    let job_id = state
        .start_import(ImportRequest {
            path: source.display().to_string(),
        })
        .expect("import starts");
    let job = wait(state, &job_id).await;
    assert_eq!(job.status, atom_core::JobStatus::Succeeded, "{:?}", job.error);
    state.models().into_iter().next().expect("model indexed")
}

fn write_quantized_template(dir: &Path) -> PathBuf {
    let path = dir.join("q4_k_m.gguf");
    std::fs::write(&path, tiny_model(GGML_TYPE_Q4_K)).unwrap();
    path
}

#[tokio::test]
async fn quantization_end_to_end_validates_and_indexes_a_new_model() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_stubs(dir.path());
    assert!(state.tool().available, "stub llama-quantize is detected");
    assert!(
        state.tool().supported_types.iter().any(|t| t == "Q4_K_M"),
        "formats are read from the stub's help output: {:?}",
        state.tool().supported_types
    );

    let source = import_padded_model(&state, &incoming).await;
    assert!(matches!(source.kind, ModelKind::Original));
    let source_before = std::fs::read(&source.path).unwrap();

    let template = write_quantized_template(dir.path());
    std::env::set_var("ATOM_STUB_MODE", "ok");
    std::env::set_var("ATOM_STUB_GGUF", &template);
    std::env::set_var("ATOM_STUB_TOTAL", "5");

    let job_id = state
        .start_quantize(QuantizeRequest {
            model_id: source.id.clone(),
            format: "Q4_K_M".into(),
            output_name: Some("base-q4_k_m".into()),
        })
        .expect("quantize starts with the stub tool");
    let job = wait(&state, &job_id).await;
    assert_eq!(job.status, atom_core::JobStatus::Succeeded, "{:?}", job.error);

    // The library now holds the source plus a new compressed variant.
    let models = state.models();
    assert_eq!(models.len(), 2, "exactly one new model is indexed");
    let compressed = models
        .iter()
        .find(|m| m.id != source.id)
        .expect("the quantized variant is indexed");
    match &compressed.kind {
        ModelKind::Compressed {
            source_model_id,
            format,
            source_sha256,
            output_sha256,
            ..
        } => {
            assert_eq!(source_model_id, &source.id);
            assert_eq!(format, "Q4_K_M");
            assert_eq!(source_sha256.as_deref(), source.sha256.as_deref());
            assert_eq!(compressed.sha256.as_deref(), Some(output_sha256.as_str()));
        }
        other => panic!("new model should be a compressed variant, got {other:?}"),
    }
    assert!(Path::new(&compressed.path).is_file());
    assert!(
        compressed.size_bytes < source.size_bytes,
        "compressed output must be smaller than the source"
    );
    assert_eq!(
        compressed.summary.label_quant(),
        "Q4_K",
        "tensor mix comes from parsing the produced file"
    );

    // Validation is reported per check and every required check passed.
    let validation = job.result.as_ref().and_then(|r| r.get("validation")).expect("validation report");
    assert_eq!(validation["ok"], serde_json::json!(true));
    let checks = validation["checks"].as_array().unwrap();
    assert!(checks.iter().any(|c| c["name"] == "output is a readable GGUF" && c["passed"] == true));
    assert!(checks.iter().any(|c| c["name"] == "tensor count matches source" && c["passed"] == true));
    assert!(checks.iter().any(|c| c["name"] == "output checksum recorded" && c["passed"] == true));

    // A run log is written next to the output, and the source is byte-identical.
    let log_path = job.result.as_ref().and_then(|r| r.get("log_path")).and_then(|v| v.as_str());
    assert!(log_path.map(|p| Path::new(p).is_file()).unwrap_or(false), "run log exists");
    assert_eq!(std::fs::read(&source.path).unwrap(), source_before, "source untouched");
}

#[tokio::test]
async fn invalid_tool_output_is_rejected_and_never_enters_the_library() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_stubs(dir.path());
    let source = import_padded_model(&state, &incoming).await;

    let garbage = dir.path().join("garbage.gguf");
    std::fs::write(&garbage, b"this is not a gguf file").unwrap();
    std::env::set_var("ATOM_STUB_MODE", "ok");
    std::env::set_var("ATOM_STUB_GGUF", &garbage);

    let job_id = state
        .start_quantize(QuantizeRequest {
            model_id: source.id.clone(),
            format: "Q4_K_M".into(),
            output_name: Some("broken".into()),
        })
        .expect("job starts");
    let job = wait(&state, &job_id).await;

    assert_eq!(job.status, atom_core::JobStatus::Failed);
    let error = job.error.unwrap_or_default();
    assert!(error.contains("failed validation"), "unexpected error: {error}");
    assert_eq!(state.models().len(), 1, "nothing is added to the library");

    // Neither the bogus output nor its run log is left behind as if it worked.
    let outputs = dir.path().join("compressed");
    let leftovers: Vec<PathBuf> = std::fs::read_dir(&outputs)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "compressed dir should be empty, found {leftovers:?}");
    assert!(Path::new(&source.path).is_file());

    std::env::remove_var("ATOM_STUB_GGUF");
}

#[tokio::test]
async fn runtime_load_streams_real_tokens_and_unloads() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_TOKENS", "8");
    std::env::set_var("ATOM_STUB_DELAY_MS", "15");
    std::env::set_var("ATOM_STUB_LOAD_MS", "120");
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_stubs(dir.path());
    let model = import_padded_model(&state, &incoming).await;

    let loaded = state
        .load_model(LoadRequest {
            model_id: model.id.clone(),
            context_length: Some(512),
            threads: Some(2),
            batch_size: Some(32),
            gpu_layers: Some(0),
            use_mmap: Some(true),
            use_mlock: Some(false),
            flash_attention: Some(false),
            kv_cache_type_k: None,
            kv_cache_type_v: None,
            memory_budget_mb: None,
        })
        .await
        .expect("stub server loads");
    assert!(loaded.status.loaded);
    assert!(loaded.status.load_ms.unwrap_or(0) >= 100, "load time is measured");
    assert_eq!(loaded.status.backend_version.as_deref(), Some("stub-0000"));
    let pid = loaded.status.pid.expect("pid recorded");

    let mut rx = state
        .submit_chat(ChatSubmission {
            conversation_id: None,
            messages: vec![atom_inference::ChatMessage {
                role: "user".into(),
                content: "What is 2 + 2?".into(),
            }],
            max_tokens: Some(32),
            temperature: Some(0.0),
            top_p: None,
            seed: Some(1),
            stop: None,
        })
        .expect("chat starts on the loaded runtime");

    let mut text = String::new();
    let mut done = false;
    while let Some(event) = rx.recv().await {
        match event {
            atom_inference::ChatEvent::Delta { text: delta } => text.push_str(&delta),
            atom_inference::ChatEvent::Done { completion_tokens, .. } => {
                assert_eq!(completion_tokens, 8);
                done = true;
            }
            atom_inference::ChatEvent::Error { message } => panic!("stream error: {message}"),
            _ => {}
        }
    }
    assert!(done, "the stream ends with a Done event");
    assert_eq!(text, "The answer is four, because 2 + 2 = 4.");

    // The engine records the measured stats and persists the assistant reply.
    let mut stats = state.last_inference();
    for _ in 0..100 {
        if stats.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        stats = state.last_inference();
    }
    let stats = stats.expect("last inference is recorded");
    assert!(stats.tokens_per_second > 0.0);
    let chats = state.inner.chats.read().unwrap();
    let convo = chats.first().expect("conversation saved");
    assert!(convo.messages.iter().any(|m| m.role == "assistant" && !m.content.is_empty()));
    drop(chats);

    state.unload_model().await.expect("unload reaps the child");
    let status = state.runtime_status().await;
    assert!(!status.loaded);
    if cfg!(target_os = "linux") {
        assert!(!Path::new(&format!("/proc/{pid}")).exists(), "child process is gone");
    }

    for key in ["ATOM_STUB_TOKENS", "ATOM_STUB_DELAY_MS", "ATOM_STUB_LOAD_MS"] {
        std::env::remove_var(key);
    }
}

#[tokio::test]
async fn engine_chat_cancellation_keeps_partial_text_and_marks_it() {
    let _guard = env_lock();
    std::env::set_var("ATOM_STUB_TOKENS", "80");
    std::env::set_var("ATOM_STUB_DELAY_MS", "30");
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_stubs(dir.path());
    let model = import_padded_model(&state, &incoming).await;

    state
        .load_model(LoadRequest {
            model_id: model.id.clone(),
            context_length: Some(256),
            threads: Some(2),
            batch_size: Some(32),
            gpu_layers: Some(0),
            use_mmap: None,
            use_mlock: None,
            flash_attention: None,
            kv_cache_type_k: None,
            kv_cache_type_v: None,
            memory_budget_mb: None,
        })
        .await
        .expect("loads");

    let mut rx = state
        .submit_chat(ChatSubmission {
            conversation_id: None,
            messages: vec![atom_inference::ChatMessage {
                role: "user".into(),
                content: "Count something long".into(),
            }],
            max_tokens: Some(64),
            temperature: None,
            top_p: None,
            seed: None,
            stop: None,
        })
        .expect("chat starts");

    // Let a few tokens through, then cancel exactly like the stop button.
    let mut seen = 0;
    while seen < 2 {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(atom_inference::ChatEvent::Delta { .. })) => seen += 1,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => panic!("no tokens streamed"),
        }
    }
    assert!(state.cancel_chat());

    let mut cancelled = false;
    while let Some(event) = rx.recv().await {
        if let atom_inference::ChatEvent::Cancelled { tokens } = event {
            assert!(tokens >= 2);
            cancelled = true;
        }
    }
    assert!(cancelled, "the UI receives a Cancelled event");

    // The engine keeps the partial answer but marks it, and records no metrics.
    let mut marked = false;
    for _ in 0..100 {
        let chats = state.inner.chats.read().unwrap();
        if let Some(convo) = chats.first() {
            if let Some(last) = convo.messages.last() {
                if last.role == "assistant" && last.content.ends_with('…') {
                    marked = true;
                    assert!(convo.metrics.is_empty(), "cancelled runs record no throughput");
                }
            }
        }
        drop(chats);
        if marked {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(marked, "partial answer is kept and marked with an ellipsis");

    state.unload_model().await.unwrap();
    for key in ["ATOM_STUB_TOKENS", "ATOM_STUB_DELAY_MS"] {
        std::env::remove_var(key);
    }
}

/// The format list used by the UI must reflect the stub's advertised set, so a
/// build without Q8_K does not offer it.
#[tokio::test]
async fn tool_formats_are_read_from_the_binary_not_hardcoded() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _incoming) = state_with_stubs(dir.path());
    let tool = state.tool();
    assert!(tool.available);
    assert!(tool.formats_from_tool, "help output was parsed");
    assert!(tool.allow_requantize, "--allow-requantize is advertised");
    assert!(!tool.supported_types.iter().any(|t| t == "Q8_K"));
}
