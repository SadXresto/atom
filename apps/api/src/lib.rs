//! HTTP surface for ATOM. The Tauri shell and the browser UI use the same routes.

use atom_bench::{all_cases, suites, ComparisonSummary};
use atom_core::{AtomConfig, AtomError, JobStatus};
use atom_engine::{AppState, ChatSubmission, LoadRequest};
use atom_inference::{ChatEvent, LoadOptions};
use atom_memory::Snapshot;
use atom_models::{GgufFile, ModelSummary};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
mod micro;

use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub struct ApiError(pub AtomError);

impl From<AtomError> for ApiError {
    fn from(e: AtomError) -> Self {
        ApiError(e)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError(e.into())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0)).into_response()
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

#[derive(Deserialize)]
struct ConfirmQuery {
    confirm: Option<String>,
    delete_file: Option<bool>,
}

pub fn router(state: AppState) -> Router {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist");
    let api = Router::new()
        .route("/health", get(health))
        .route("/overview", get(overview))
        .route("/system", get(system))
        .route("/system/history", get(system_history))
        .route("/system/stream", get(system_stream))
        .route("/logs", get(logs))
        .route("/backend", get(backend))
        .route("/backend/refresh", post(refresh_backend))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/models", get(list_models).post(import_model))
        .route("/models/upload", post(upload_model))
        .route("/models/inspect", post(inspect_path))
        .route("/models/{id}", get(get_model).delete(delete_model))
        .route("/models/{id}/verify", post(verify_model))
        .route("/models/{id}/quantize/options", get(quantize_options))
        .route("/models/{id}/quantize", post(start_quantize))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/logs", get(job_logs))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}/stream", get(job_stream))
        .route("/runtime", get(runtime_status))
        .route("/runtime/load", post(load_model))
        .route("/runtime/unload", post(unload_model))
        .route("/runtime/release", post(release_resources))
        .route("/chat", post(chat))
        .route("/chat/cancel", post(cancel_chat))
        .route("/conversations", get(list_conversations))
        .route("/conversations/{id}", get(get_conversation).delete(delete_conversation))
        .route("/conversations/{id}/export", get(export_conversation))
        .route("/comparisons", get(list_comparisons).post(start_comparison))
        .route("/comparisons/{id}", get(get_comparison).delete(delete_comparison))
        .route("/comparisons/{id}/report", get(comparison_report))
        .route("/bench/suites", get(bench_suites))
        .merge(micro::router())
        .with_state(state)
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024));

    let app = Router::new().nest("/api", api);
    if dist.is_dir() {
        let index = dist.join("index.html");
        let serve = tower_http::services::ServeDir::new(&dist)
            .fallback(tower_http::services::ServeFile::new(index));
        app.fallback_service(serve)
    } else {
        app.fallback(get(missing_frontend))
    }
}

async fn missing_frontend() -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "name": "ATOM",
            "message": "The frontend has not been built yet. Run `npm install && npm run build` in frontend/, or start the dev server on port 5173.",
            "api": "/api/health"
        })),
    )
        .into_response()
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "name": "atom", "version": env!("CARGO_PKG_VERSION") }))
}

async fn overview(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let runtime = state.runtime_status().await;
    let snapshot = state.snapshot().await;
    let models = state.models();
    let disk: u64 = models.iter().map(|m| m.size_bytes).sum();
    let jobs: Vec<Value> = state
        .inner
        .jobs
        .list()
        .into_iter()
        .take(6)
        .map(|j| serde_json::to_value(j).unwrap_or(Value::Null))
        .collect();
    let comparisons: Vec<ComparisonSummary> = state.inner.comparisons.list().into_iter().take(3).collect();
    let backend = state.backend();
    let last = state.last_inference();
    Ok(Json(json!({
        "runtime": runtime,
        "system": snapshot,
        "backend": backend,
        "tool": state.tool(),
        "counts": {
            "models": models.len(),
            "originals": models.iter().filter(|m| !m.is_compressed()).count(),
            "compressed": models.iter().filter(|m| m.is_compressed()).count(),
            "jobs": state.inner.jobs.list().len(),
            "comparisons": state.inner.comparisons.list().len(),
        },
        "disk": {
            "library_bytes": disk,
            "compressed_bytes": models.iter().filter(|m| m.is_compressed()).map(|m| m.size_bytes).sum::<u64>(),
        },
        "last_inference": last,
        "recent_jobs": jobs,
        "recent_comparisons": comparisons,
        "started_at": state.inner.started_at.to_rfc3339(),
    })))
}

async fn system(State(state): State<AppState>) -> Json<Snapshot> {
    Json(state.snapshot().await)
}

async fn system_history(State(state): State<AppState>) -> Json<Value> {
    let history = state.history();
    Json(json!({ "samples": history }))
}

async fn system_stream(State(state): State<AppState>) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let (tx, rx) = mpsc::channel::<Event>(64);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(1500));
        loop {
            ticker.tick().await;
            let snapshot = state.snapshot().await;
            let event = Event::default()
                .event("snapshot")
                .json_data(&snapshot)
                .unwrap_or_else(|_| Event::default().comment("encode error"));
            if tx.send(event).await.is_err() {
                break;
            }
        }
    });
    Sse::new(ReceiverStream::new(rx).map(Ok)).keep_alive(KeepAlive::default())
}

async fn logs(State(state): State<AppState>, Query(q): Query<LimitQuery>) -> Json<Value> {
    let limit = q.limit.unwrap_or(200).min(2000);
    Json(json!({ "lines": state.inner.logbuf.tail(limit) }))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

async fn backend(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "backend": state.backend(), "tool": state.tool() }))
}

async fn refresh_backend(State(state): State<AppState>) -> Json<Value> {
    state.refresh_backend();
    Json(json!({ "backend": state.backend(), "tool": state.tool() }))
}

async fn get_settings(State(state): State<AppState>) -> Json<AtomConfig> {
    Json(state.config())
}

async fn put_settings(State(state): State<AppState>, Json(next): Json<AtomConfig>) -> ApiResult<Json<AtomConfig>> {
    Ok(Json(state.update_config(next)?))
}

async fn list_models(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "models": state.models() }))
}

async fn get_model(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let model = state
        .model(&id)
        .ok_or_else(|| AtomError::NotFound(format!("model {id} is not in the library")))?;
    let variants = state.library_variants(&id);
    let source = state
        .inner
        .library
        .read()
        .ok()
        .and_then(|lib| lib.source_of(&model).cloned());
    Ok(Json(json!({ "model": model, "variants": variants, "source": source })))
}

#[derive(Deserialize)]
struct ImportBody {
    path: String,
}

async fn import_model(State(state): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult<Json<Value>> {
    let job = state.start_import(atom_engine::jobs::ImportRequest { path: body.path })?;
    Ok(Json(json!({ "job_id": job })))
}

async fn inspect_path(State(state): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult<Json<Value>> {
    let guard = state.guard();
    let path = guard.resolve_existing(&body.path)?;
    let gguf = GgufFile::open(&path)?;
    let summary = ModelSummary::from_gguf(&gguf, 0);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let tool = state.tool();
    let formats = atom_compression::formats_for(&tool, &summary, size);
    Ok(Json(json!({
        "path": path.display().to_string(),
        "size_bytes": size,
        "summary": summary,
        "formats": formats,
        "tool": tool,
    })))
}

async fn upload_model(State(state): State<AppState>, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    let cfg = state.config();
    let models_dir = PathBuf::from(&cfg.models_dir);
    std::fs::create_dir_all(&models_dir)?;
    let mut saved: Option<PathBuf> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AtomError::Invalid(format!("upload failed: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let name = field
            .file_name()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "upload.gguf".to_string());
        let file_name = std::path::Path::new(&name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload.gguf".into());
        atom_security::validate_file_name(&file_name)?;
        if !file_name.to_lowercase().ends_with(".gguf") {
            return Err(AtomError::Invalid("only .gguf files can be imported".into()).into());
        }
        let target = models_dir.join(format!("upload-{}", file_name));
        let mut file = tokio::fs::File::create(&target).await?;
        use tokio::io::AsyncWriteExt;
        let mut written: u64 = 0;
        let mut field = field;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AtomError::Invalid(format!("upload interrupted: {e}")))?
        {
            written += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        if written == 0 {
            return Err(AtomError::Invalid("uploaded file was empty".into()).into());
        }
        saved = Some(target);
    }
    let path = saved.ok_or_else(|| AtomError::Invalid("no file field in the upload".into()))?;
    let job = state.start_import(atom_engine::jobs::ImportRequest {
        path: path.display().to_string(),
    })?;
    Ok(Json(json!({ "job_id": job, "staged_path": path.display().to_string() })))
}

async fn delete_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ConfirmQuery>,
) -> ApiResult<Json<Value>> {
    if q.confirm.as_deref() != Some(id.as_str()) {
        return Err(AtomError::Forbidden(
            "deleting a model requires confirm=<model id> to acknowledge the action".into(),
        )
        .into());
    }
    let record = state
        .model(&id)
        .ok_or_else(|| AtomError::NotFound(format!("model {id} is not in the library")))?;
    let remove_file = q.delete_file.unwrap_or(false);
    let path = PathBuf::from(&record.path);
    if remove_file && path.exists() {
        let guard = state.guard();
        let resolved = guard.resolve_existing(&path)?;
        std::fs::remove_file(resolved)?;
    }
    if let Ok(mut lib) = state.inner.library.write() {
        lib.remove(&id)?;
    }
    state.log(
        "warn",
        "library",
        format!(
            "removed {} from the library{}",
            record.label,
            if remove_file { " and deleted its file" } else { "" }
        ),
    );
    Ok(Json(json!({ "removed": id, "file_deleted": remove_file })))
}

async fn verify_model(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let job = state.start_verify(&id)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn quantize_options(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let model = state
        .model(&id)
        .ok_or_else(|| AtomError::NotFound(format!("model {id} is not in the library")))?;
    let tool = state.tool();
    let formats = atom_compression::formats_for(&tool, &model.summary, model.size_bytes);
    Ok(Json(json!({
        "model_id": id,
        "formats": formats,
        "tool": tool,
        "compatibility": model.summary.compatibility,
        "source_size": model.size_bytes,
    })))
}

async fn start_quantize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let format = body
        .get("format")
        .and_then(|f| f.as_str())
        .ok_or_else(|| AtomError::Invalid("format is required".into()))?;
    let output_name = body.get("output_name").and_then(|f| f.as_str()).map(|s| s.to_string());
    let job = state.start_quantize(atom_engine::jobs::QuantizeRequest {
        model_id: id,
        format: format.to_string(),
        output_name,
    })?;
    Ok(Json(json!({ "job_id": job })))
}

async fn list_jobs(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "jobs": state.inner.jobs.list() }))
}

async fn get_job(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let job = state
        .inner
        .jobs
        .get(&id)
        .ok_or_else(|| AtomError::NotFound(format!("job {id} not found")))?;
    Ok(Json(json!({ "job": job })))
}

async fn job_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AfterQuery>,
) -> ApiResult<Json<Value>> {
    if state.inner.jobs.get(&id).is_none() {
        return Err(AtomError::NotFound(format!("job {id} not found")).into());
    }
    let logs = state.inner.jobs.logs(&id, q.after_seq.unwrap_or(0));
    Ok(Json(json!({ "logs": logs })))
}

#[derive(Deserialize)]
struct AfterQuery {
    after_seq: Option<u64>,
}

async fn cancel_job(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let ok = state.inner.jobs.request_cancel(&id);
    if !ok {
        return Err(AtomError::NotFound(format!("job {id} not found")).into());
    }
    Ok(Json(json!({ "cancelling": id })))
}

async fn job_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let job = state
        .inner
        .jobs
        .get(&id)
        .ok_or_else(|| AtomError::NotFound(format!("job {id} not found")))?;
    let (tx, rx) = mpsc::channel::<Event>(256);
    let mut bus = state.inner.jobs.subscribe();
    tokio::spawn(async move {
        let _ = tx
            .send(Event::default().event("state").json_data(&job).unwrap_or_default())
            .await;
        match job.status {
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled => return,
            _ => {}
        }
        loop {
            match bus.recv().await {
                Ok(event) => {
                    if event.job_id() != id {
                        continue;
                    }
                    let terminal = matches!(
                        &event,
                        atom_core::jobs::JobEvent::Status { status, .. } if status.is_terminal()
                    );
                    let out = Event::default()
                        .event("job")
                        .json_data(&event)
                        .unwrap_or_else(|_| Event::default().comment("encode error"));
                    if tx.send(out).await.is_err() {
                        break;
                    }
                    if terminal {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
    Ok(Sse::new(ReceiverStream::new(rx).map(Ok)).keep_alive(KeepAlive::default()))
}

async fn runtime_status(State(state): State<AppState>) -> Json<Value> {
    let runtime = state.runtime_status().await;
    let tuning = runtime
        .options
        .as_ref()
        .map(|o| LoadOptions::tuning_summary(o))
        .unwrap_or_default();
    Json(json!({ "runtime": runtime, "tuning": tuning }))
}

async fn load_model(State(state): State<AppState>, Json(body): Json<LoadRequest>) -> ApiResult<Json<Value>> {
    let outcome = state.load_model(body).await?;
    Ok(Json(serde_json::to_value(outcome).unwrap_or(Value::Null)))
}

async fn unload_model(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let status = state.unload_model().await?;
    Ok(Json(json!({ "runtime": status })))
}

async fn release_resources(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let message = state.release_resources().await?;
    Ok(Json(json!({ "released": message })))
}

async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatSubmission>,
) -> ApiResult<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let mut rx = state.submit_chat(body)?;
    let (tx, out_rx) = mpsc::channel::<Event>(128);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let terminal = matches!(
                event,
                ChatEvent::Done { .. } | ChatEvent::Cancelled { .. } | ChatEvent::Error { .. }
            );
            let out = Event::default()
                .event("chat")
                .json_data(&event)
                .unwrap_or_else(|_| Event::default().comment("encode error"));
            if tx.send(out).await.is_err() {
                break;
            }
            if terminal {
                break;
            }
        }
    });
    Ok(Sse::new(ReceiverStream::new(out_rx).map(Ok)).keep_alive(KeepAlive::default()))
}

async fn cancel_chat(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "cancelled": state.cancel_chat() }))
}

async fn list_conversations(State(state): State<AppState>) -> Json<Value> {
    let chats = state
        .inner
        .chats
        .read()
        .map(|c| c.clone())
        .unwrap_or_default();
    let mut chats = chats;
    // Newest first: the inference workspace uses the head of this list to
    // resolve the conversation it just created.
    chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let summaries: Vec<Value> = chats
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "title": c.title,
                "model_label": c.model_label,
                "messages": c.messages.len(),
                "updated_at": c.updated_at,
                "metrics": c.metrics.last(),
            })
        })
        .collect();
    Json(json!({ "conversations": summaries }))
}

async fn get_conversation(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let chats = state.inner.chats.read().map(|c| c.clone()).unwrap_or_default();
    let convo = chats
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AtomError::NotFound("conversation not found".into()))?;
    Ok(Json(json!({ "conversation": convo })))
}

async fn delete_conversation(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut chats = state
        .inner
        .chats
        .write()
        .map_err(|_| AtomError::Internal("chat store poisoned".into()))?;
    let before = chats.len();
    chats.retain(|c| c.id != id);
    if chats.len() == before {
        return Err(AtomError::NotFound("conversation not found".into()).into());
    }
    atom_engine::state::save_conversations(&state.paths().chat_dir(), &chats);
    Ok(Json(json!({ "deleted": id })))
}

async fn export_conversation(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let chats = state.inner.chats.read().map(|c| c.clone()).unwrap_or_default();
    let convo = chats
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AtomError::NotFound("conversation not found".into()))?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        convo.to_markdown(),
    )
        .into_response())
}

async fn list_comparisons(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "comparisons": state.inner.comparisons.list(), "suites": suites() }))
}

async fn bench_suites() -> Json<Value> {
    Json(json!({
        "suites": suites(),
        "cases": all_cases().into_iter().map(|c| json!({
            "id": c.id, "suite": c.suite, "title": c.title, "prompt": c.prompt, "note": c.note
        })).collect::<Vec<_>>()
    }))
}

async fn start_comparison(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    let request: atom_engine::jobs::ComparisonRequest = serde_json::from_value(body)
        .map_err(|e| AtomError::Invalid(format!("invalid comparison request: {e}")))?;
    let job = state.start_comparison(request)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn get_comparison(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let cmp = state.inner.comparisons.load(&id)?;
    Ok(Json(json!({ "comparison": cmp })))
}

async fn delete_comparison(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    state.inner.comparisons.delete(&id)?;
    Ok(Json(json!({ "deleted": id })))
}

async fn comparison_report(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Response> {
    let cmp = state.inner.comparisons.load(&id)?;
    let markdown = atom_bench::render_markdown(&cmp);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        markdown,
    )
        .into_response())
}
