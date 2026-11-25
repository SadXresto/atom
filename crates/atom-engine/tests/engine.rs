//! End-to-end checks for the composition root: real jobs, real files, real
//! refusal when the backend is missing.

use atom_core::JobStatus;
use atom_engine::jobs::{ComparisonRequest, ImportRequest, QuantizeRequest};
use atom_engine::AppState;
use atom_models::gguf::fixture::tiny_model;
use atom_models::quant::GGML_TYPE_F16;
use std::path::PathBuf;
use std::time::Duration;

fn state_in(dir: &std::path::Path) -> AppState {
    AppState::new(atom_core::Paths::from_root(dir.to_path_buf())).expect("engine starts")
}

/// An engine whose allowlist includes a local `incoming` directory, mirroring a
/// developer adding their downloads folder in Settings.
fn state_with_incoming(dir: &std::path::Path) -> (AppState, PathBuf) {
    let incoming = dir.join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let paths = atom_core::Paths::from_root(dir.to_path_buf());
    let mut config = atom_core::AtomConfig::load(&paths).unwrap();
    config.model_roots.push(incoming.display().to_string());
    config.save(&paths).unwrap();
    (AppState::new(paths).expect("engine starts"), incoming)
}

async fn wait(state: &AppState, job_id: &str) -> atom_core::JobView {
    for _ in 0..200 {
        if let Some(job) = state.inner.jobs.get(job_id) {
            if job.status.is_terminal() {
                return job;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job {job_id} did not finish in time");
}

#[tokio::test]
async fn import_job_indexes_a_real_gguf() {
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_incoming(dir.path());
    let source = incoming.join("incoming.gguf");
    std::fs::write(&source, tiny_model(GGML_TYPE_F16)).unwrap();

    let job_id = state
        .start_import(ImportRequest {
            path: source.display().to_string(),
        })
        .expect("import starts");
    let job = wait(&state, &job_id).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let models = state.models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].summary.architecture.as_deref(), Some("llama"));
    assert!(models[0].sha256.is_some(), "checksum recorded");
    assert!(std::path::Path::new(&models[0].path).is_file());
    // The library index is persisted next to the config.
    assert!(dir.path().join("library.json").is_file());
    // The imported copy is not the source path.
    assert_ne!(models[0].path, source.display().to_string());
}

#[tokio::test]
async fn import_rejects_files_outside_allowed_roots() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_in(dir.path());
    let outside = tempfile::tempdir().unwrap();
    let source = outside.path().join("bad.gguf");
    std::fs::write(&source, tiny_model(GGML_TYPE_F16)).unwrap();

    let job_id = state
        .start_import(ImportRequest {
            path: source.display().to_string(),
        })
        .expect("job is created and then fails");
    let job = wait(&state, &job_id).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert!(
        job.error.unwrap_or_default().contains("outside"),
        "expected an outside-roots error"
    );
    assert!(state.models().is_empty());
}

#[tokio::test]
async fn quantization_refuses_without_tool_and_never_creates_output() {
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_incoming(dir.path());
    let source = incoming.join("incoming.gguf");
    std::fs::write(&source, tiny_model(GGML_TYPE_F16)).unwrap();
    let job_id = state
        .start_import(ImportRequest {
            path: source.display().to_string(),
        })
        .expect("import starts");
    let imported = wait(&state, &job_id).await;
    assert_eq!(imported.status, JobStatus::Succeeded, "{:?}", imported.error);
    let model = state.models().into_iter().next().unwrap();

    // No llama-quantize in this environment: ATOM must refuse, not simulate.
    let result = state.start_quantize(QuantizeRequest {
        model_id: model.id.clone(),
        format: "Q4_K_M".into(),
        output_name: None,
    });
    if state.tool().available {
        // A developer machine with llama.cpp may legitimately start the job.
        assert!(result.is_ok());
    } else {
        let err = result.expect_err("compression must be unavailable");
        assert_eq!(err.code(), "unavailable");
        let outputs = dir.path().join("compressed");
        let entries: Vec<PathBuf> = std::fs::read_dir(&outputs)
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(entries.is_empty(), "no output may be produced");
    }
}

#[tokio::test]
async fn comparison_requires_inference_backend() {
    let dir = tempfile::tempdir().unwrap();
    let (state, incoming) = state_with_incoming(dir.path());
    let source = incoming.join("incoming.gguf");
    std::fs::write(&source, tiny_model(GGML_TYPE_F16)).unwrap();
    let job_id = state
        .start_import(ImportRequest {
            path: source.display().to_string(),
        })
        .expect("import starts");
    let imported = wait(&state, &job_id).await;
    assert_eq!(imported.status, JobStatus::Succeeded, "{:?}", imported.error);
    let model = state.models().into_iter().next().unwrap();

    let result = state.start_comparison(ComparisonRequest {
        model_ids: vec![model.id],
        suite_ids: vec!["math".into()],
        settings: None,
        title: None,
    });
    if state.backend().inference_ready {
        assert!(result.is_ok());
    } else {
        assert_eq!(result.expect_err("backend missing").code(), "unavailable");
    }
}

#[tokio::test]
async fn destructive_model_actions_require_engine_consent() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_in(dir.path());
    assert!(state.unload_model().await.is_err(), "nothing to unload");
    // Release is always safe and never deletes files.
    assert!(state.release_resources().await.is_ok());
    let models_dir = dir.path().join("models");
    assert!(models_dir.is_dir());
}

#[test]
fn config_roundtrip_keeps_paths() {
    let dir = tempfile::tempdir().unwrap();
    let paths = atom_core::Paths::from_root(dir.path().to_path_buf());
    let mut config = atom_core::AtomConfig::load(&paths).unwrap();
    config.threads = 6;
    config.save(&paths).unwrap();
    let reloaded = atom_core::AtomConfig::load(&paths).unwrap();
    assert_eq!(reloaded.threads, 6);
    assert_eq!(reloaded.models_dir, config.models_dir);
}
