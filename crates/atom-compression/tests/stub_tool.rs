//! Integration tests for the quantization runner against a stub
//! `llama-quantize` executable.
//!
//! These prove the things that unit tests cannot: the child process is really
//! spawned, its tensor-progress lines really move the job forward, cancellation
//! really kills the process and deletes the partial output, and a failed or
//! empty run really cleans up.

use atom_compression::{run_quantize, QuantizePlan};
use atom_core::{JobKind, JobStore};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// The stub reads its mode from the environment, so tests that change it must
/// not run concurrently within this binary.
static ENV: Mutex<()> = Mutex::new(());

fn env_lock() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn stub() -> PathBuf {
    atom_teststubs::bin("stub-llama-quantize")
}

fn set_stub(mode: &str, template: Option<&Path>, total: u64) {
    std::env::set_var("ATOM_STUB_MODE", mode);
    std::env::set_var("ATOM_STUB_TOTAL", total.to_string());
    match template {
        Some(path) => std::env::set_var("ATOM_STUB_GGUF", path),
        None => std::env::remove_var("ATOM_STUB_GGUF"),
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    source: PathBuf,
    output: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.gguf");
    std::fs::write(&source, atom_models::gguf::fixture::tiny_model(atom_models::quant::GGML_TYPE_F16))
        .unwrap();
    let output = dir.path().join("source-q4_k_m.gguf");
    Fixture { dir, source, output }
}

fn plan(f: &Fixture) -> QuantizePlan {
    QuantizePlan {
        tool_path: stub().display().to_string(),
        source_path: f.source.display().to_string(),
        output_path: f.output.display().to_string(),
        format: "Q4_K_M".into(),
        argv: vec![
            "-t".into(),
            "2".into(),
            f.source.display().to_string(),
            f.output.display().to_string(),
            "Q4_K_M".into(),
        ],
        threads: 2,
        allow_requantize: false,
    }
}

fn job(store: &JobStore) -> (String, atom_core::JobHandle) {
    let (id, handle) = store.create(JobKind::Quantize, "stub quantization", serde_json::json!({}));
    handle.start();
    (id, handle)
}

#[tokio::test]
async fn real_child_progress_moves_the_job_and_output_is_written() {
    let _guard = env_lock();
    let f = fixture();
    // The stub copies this file to the output path, exactly like a tool writing
    // a finished GGUF.
    let template = f.dir.path().join("template.gguf");
    std::fs::write(
        &template,
        atom_models::gguf::fixture::tiny_model(atom_models::quant::GGML_TYPE_Q4_K),
    )
    .unwrap();
    set_stub("ok", Some(&template), 6);

    let store = JobStore::new();
    let (id, handle) = job(&store);
    let outcome = run_quantize(&handle, &plan(&f)).await.expect("stub run succeeds");

    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.format, "Q4_K_M");
    assert_eq!(outcome.produced_bytes, std::fs::metadata(&template).unwrap().len());
    assert!(f.output.is_file());

    let view = store.get(&id).expect("job is registered");
    assert!(
        view.progress >= 0.97,
        "progress should reach the validation stage, got {}",
        view.progress
    );
    assert_eq!(view.stage, "quantization finished, validating output");

    let logs = store.logs(&id, 0);
    // Every tensor line the tool printed is in the job log, and the last one to
    // arrive was parsed into progress rather than padded with a guess.
    assert!(
        logs.iter().any(|l| l.message.contains("[   6/6]")),
        "expected the tool's own progress line in the log: {:?}",
        logs.iter().map(|l| &l.message).collect::<Vec<_>>()
    );
    assert!(logs.iter().any(|l| l.stream == "stdout"));
}

#[tokio::test]
async fn cancellation_kills_the_child_and_removes_partial_output() {
    let _guard = env_lock();
    let f = fixture();
    set_stub("hang", None, 8);

    let source_before = std::fs::read(&f.source).unwrap();
    let store = JobStore::new();
    let (id, handle) = job(&store);
    let worker = {
        let handle = handle.clone();
        let plan = plan(&f);
        tokio::spawn(async move { run_quantize(&handle, &plan).await })
    };

    // The stub writes a partial file before it starts "working".
    for _ in 0..200 {
        if f.output.is_file() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(f.output.is_file(), "stub should have created a partial output");

    // Cancel through the registry, the way the API does.
    assert!(store.request_cancel(&id));

    let result = tokio::time::timeout(Duration::from_secs(15), worker)
        .await
        .expect("cancellation must not hang")
        .expect("worker task did not panic");
    let err = result.expect_err("a cancelled quantization must not report success");
    assert_eq!(err.code(), "cancelled");

    assert!(!f.output.exists(), "partial output must be deleted on cancel");
    assert_eq!(std::fs::read(&f.source).unwrap(), source_before, "source must be untouched");
    let logs = store.logs(&id, 0);
    assert!(
        logs.iter().any(|l| l.message.contains("partial output removed")),
        "cancellation must be logged: {:?}",
        logs.iter().map(|l| &l.message).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn tool_failure_removes_partial_output_and_reports_exit_code() {
    let _guard = env_lock();
    let f = fixture();
    set_stub("fail", None, 3);

    let store = JobStore::new();
    let (_id, handle) = job(&store);
    let err = run_quantize(&handle, &plan(&f)).await.expect_err("exit 3 is a failure");

    let message = err.to_string();
    assert!(message.contains("exited with 3"), "unexpected message: {message}");
    assert!(!f.output.exists(), "failed output must be deleted");
    assert!(f.source.is_file(), "source must be untouched");
}

#[tokio::test]
async fn success_without_output_is_rejected() {
    let _guard = env_lock();
    let f = fixture();
    set_stub("missing", None, 2);

    let store = JobStore::new();
    let (_id, handle) = job(&store);
    let err = run_quantize(&handle, &plan(&f)).await.expect_err("no output file");
    assert!(err.to_string().contains("produced no output file"), "{err}");
    assert!(!f.output.exists());
}

#[tokio::test]
async fn empty_output_is_rejected() {
    let _guard = env_lock();
    let f = fixture();
    set_stub("empty", None, 2);

    let store = JobStore::new();
    let (_id, handle) = job(&store);
    let err = run_quantize(&handle, &plan(&f)).await.expect_err("empty output");
    assert!(err.to_string().contains("empty file"), "{err}");
    assert!(!f.output.exists());
}
