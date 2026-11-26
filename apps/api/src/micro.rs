//! HTTP surface for ATOM Micro Studio, training, distillation, hardware planning,
//! exports and firmware.

use atom_core::AtomError;
use atom_engine::classify::{
    self as classification, ClassifyImportRequest, ClassifyProfileRequest, ClassifyTrainRequest,
};
use atom_engine::micro::{self, BenchmarkRequest, DatasetRequest, DistillRequest, InferRequest, TrainRequest};
use atom_engine::AppState;
use axum::{
    extract::{Path as AxumPath, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ApiResult;

/// Project JSON plus the derived workflow position, so the UI never has to
/// reimplement the step rules.
fn project_value(project: &atom_micro::MicroProject) -> Value {
    let mut value = serde_json::to_value(project).unwrap_or(Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("next_step".into(), json!(project.next_step()));
        object.insert("completed_steps".into(), json!(project.completed_steps()));
    }
    value
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/micro/overview", get(overview))
        .route("/micro/capabilities", get(capabilities))
        .route("/micro/catalog", get(catalog))
        .route("/micro/benchmark-schema", get(benchmark_schema))
        .route("/micro/projects", get(list_projects).post(create_project))
        .route("/micro/projects/{id}", get(get_project).patch(patch_project).delete(delete_project))
        .route("/micro/projects/{id}/dataset", post(import_dataset))
        .route("/micro/projects/{id}/classification/profile", post(classify_profile))
        .route(
            "/micro/projects/{id}/classification/dataset",
            post(classify_dataset),
        )
        .route("/micro/projects/{id}/classification/train", post(classify_train))
        .route(
            "/micro/projects/{id}/classification/export",
            post(classify_export),
        )
        .route("/micro/projects/{id}/train", post(train))
        .route("/micro/projects/{id}/distill", post(distill))
        .route("/micro/projects/{id}/evaluate", post(evaluate))
        .route("/micro/projects/{id}/quantize", post(quantize))
        .route("/micro/projects/{id}/memory", post(memory))
        .route("/micro/projects/{id}/export", post(export))
        .route("/micro/projects/{id}/firmware/build", post(build_firmware))
        .route("/micro/projects/{id}/firmware/deploy", post(deploy_firmware))
        .route("/micro/projects/{id}/benchmarks", post(import_benchmarks))
        .route("/micro/projects/{id}/infer", post(infer))
        .route("/micro/projects/{id}/files/{name}", get(download_file))
}

async fn overview(State(state): State<AppState>) -> Json<Value> {
    Json(micro::overview(&state))
}

async fn capabilities() -> Json<Value> {
    Json(serde_json::to_value(atom_micro::tools::capabilities()).unwrap_or(Value::Null))
}

async fn catalog() -> Json<Value> {
    Json(json!({
        "targets": atom_micro::hardware::targets(),
        "architectures": atom_micro::architecture::architectures(),
    }))
}

async fn benchmark_schema() -> Json<Value> {
    Json(json!({
        "schema": atom_micro::bench::schema_example(),
        "note": "Import a file that matches this shape. Every value must be measured by the device \
                 firmware; ATOM rejects zero or missing measurements."
    }))
}

async fn list_projects(State(state): State<AppState>) -> Json<Value> {
    Json(micro::overview(&state))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    domain: String,
    target_id: String,
}

async fn create_project(State(state): State<AppState>, Json(body): Json<CreateBody>) -> ApiResult<Json<Value>> {
    let project = state
        .inner
        .micro
        .create(&body.name, &body.description, &body.domain, &body.target_id)?;
    state.log("info", "micro", format!("created micro project {}", project.name));
    Ok(Json(json!({ "project": project_value(&project) })))
}

async fn get_project(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> ApiResult<Json<Value>> {
    let project = state.inner.micro.get(&id)?;
    let capabilities = atom_micro::tools::capabilities();
    Ok(Json(json!({
        "project": project_value(&project),
        "capabilities": capabilities,
        "benchmark_summary": atom_micro::bench::summarize(&project.benchmarks),
    })))
}

#[derive(Deserialize)]
struct PatchBody {
    name: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    target_id: Option<String>,
}

async fn patch_project(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<PatchBody>,
) -> ApiResult<Json<Value>> {
    let mut project = state.inner.micro.get(&id)?;
    if let Some(name) = body.name {
        if name.trim().len() < 2 {
            return Err(AtomError::Invalid("project name needs at least 2 characters".into()).into());
        }
        project.name = name.trim().to_string();
    }
    if let Some(description) = body.description {
        project.description = description;
    }
    if let Some(domain) = body.domain {
        project.domain = domain;
    }
    if let Some(target) = body.target_id {
        if atom_micro::hardware::target(&target).is_none() {
            return Err(AtomError::Invalid(format!("unknown target hardware '{target}'")).into());
        }
        if target != project.target_id {
            project.memory = None;
            project.firmware = None;
        }
        project.target_id = target;
    }
    state.inner.micro.save(&mut project)?;
    Ok(Json(json!({ "project": project_value(&project) })))
}

#[derive(Deserialize)]
struct ConfirmQuery {
    confirm: Option<String>,
}

async fn delete_project(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<ConfirmQuery>,
) -> ApiResult<Json<Value>> {
    if q.confirm.as_deref() != Some(id.as_str()) {
        return Err(AtomError::Forbidden(
            "deleting a Micro project requires confirm=<project id>; its weights and exports are removed"
                .into(),
        )
        .into());
    }
    state.inner.micro.delete(&id)?;
    state.log("warn", "micro", format!("deleted micro project {id}"));
    Ok(Json(json!({ "deleted": id })))
}

async fn import_dataset(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<DatasetRequest>,
) -> ApiResult<Json<Value>> {
    let project = micro::import_dataset(&state, &id, body)?;
    Ok(Json(json!({ "project": project_value(&project) })))
}

async fn classify_profile(
    State(state): State<AppState>,
    AxumPath(_id): AxumPath<String>,
    Json(body): Json<ClassifyProfileRequest>,
) -> ApiResult<Json<Value>> {
    let profile = classification::profile_path(&state, body)?;
    Ok(Json(json!({ "profile": profile })))
}

async fn classify_dataset(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<ClassifyImportRequest>,
) -> ApiResult<Json<Value>> {
    let project = classification::import_dataset(&state, &id, body)?;
    Ok(Json(json!({ "project": project_value(&project) })))
}

async fn classify_train(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let req: ClassifyTrainRequest = serde_json::from_value(body)
        .map_err(|e| AtomError::Invalid(format!("invalid classification request: {e}")))?;
    let job = state.start_micro_classify(&id, req)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn classify_export(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let export = classification::export_project(&state, &id)?;
    Ok(Json(json!({ "export": export })))
}

async fn train(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let req: TrainRequest = serde_json::from_value(body)
        .map_err(|e| AtomError::Invalid(format!("invalid training request: {e}")))?;
    let job = state.start_micro_train(&id, req)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn distill(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let req: DistillRequest = serde_json::from_value(body)
        .map_err(|e| AtomError::Invalid(format!("invalid distillation request: {e}")))?;
    let job = state.start_micro_distill(&id, req)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn evaluate(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> ApiResult<Json<Value>> {
    let report = micro::evaluate_project(&state, &id)?;
    Ok(Json(json!({ "evaluation": report })))
}

#[derive(Deserialize)]
struct QuantizeBody {
    format: String,
}

async fn quantize(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<QuantizeBody>,
) -> ApiResult<Json<Value>> {
    let summary = micro::quantize_project(&state, &id, &body.format)?;
    Ok(Json(json!({ "quantization": summary })))
}

#[derive(Deserialize, Default)]
struct MemoryBody {
    target_id: Option<String>,
}

async fn memory(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    body: Option<Json<MemoryBody>>,
) -> ApiResult<Json<Value>> {
    let target = body.and_then(|b| b.0.target_id);
    let plan = micro::plan_project(&state, &id, target.as_deref())?;
    Ok(Json(json!({ "memory": plan })))
}

async fn export(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> ApiResult<Json<Value>> {
    let report = micro::export_project(&state, &id)?;
    Ok(Json(json!({ "export": report })))
}

async fn build_firmware(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> ApiResult<Json<Value>> {
    let job = state.start_micro_firmware(&id, false)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn deploy_firmware(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> ApiResult<Json<Value>> {
    let job = state.start_micro_firmware(&id, true)?;
    Ok(Json(json!({ "job_id": job })))
}

async fn import_benchmarks(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<BenchmarkRequest>,
) -> ApiResult<Json<Value>> {
    let runs = micro::import_benchmarks(&state, &id, body)?;
    Ok(Json(json!({ "runs": runs })))
}

async fn infer(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<InferRequest>,
) -> ApiResult<Json<Value>> {
    let response = micro::infer_project(&state, &id, body)?;
    Ok(Json(serde_json::to_value(response).unwrap_or(Value::Null)))
}

const EXPORT_FILES: [&str; 7] = [
    "model.bin",
    "model.c",
    "model.h",
    "runtime.c",
    "main.c",
    "manifest.json",
    "README.md",
];

async fn download_file(
    State(state): State<AppState>,
    AxumPath((id, name)): AxumPath<(String, String)>,
) -> ApiResult<Response> {
    if !EXPORT_FILES.contains(&name.as_str()) {
        return Err(AtomError::Forbidden(format!("{name} is not an exportable artifact")).into());
    }
    let project = state.inner.micro.get(&id)?;
    let export = project
        .export
        .ok_or_else(|| AtomError::Conflict("export the model first".into()))?;
    let path = std::path::PathBuf::from(&export.dir).join(&name);
    let bytes = std::fs::read(&path)?;
    let content_type = if name.ends_with(".json") {
        "application/json"
    } else if name.ends_with(".md") {
        "text/markdown; charset=utf-8"
    } else if name.ends_with(".bin") {
        "application/octet-stream"
    } else {
        "text/plain; charset=utf-8"
    };
    Ok(([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response())
}
