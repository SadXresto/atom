//! Orchestration for ATOM Micro: dataset import, training jobs, evaluation,
//! quantization, export, firmware builds and teacher distillation.
//!
//! Training and firmware builds are real background jobs with cancellation.
//! Anything that needs a tool or a device ATOM does not have fails with a clear
//! message; it never reports success.

use atom_core::{AtomError, JobKind, Result};
use atom_inference::{ChatMessage, ChatRequest};
use atom_micro::{
    architecture, dataset, export, firmware, hardware, ArchSelection, ByteMlp, DistillReport,
    ExportInput, MicroProject, QuantSummary, TrainConfig, TrainingSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct TrainRequest {
    pub spec_id: String,
    pub context: Option<u32>,
    pub hidden: Option<u32>,
    pub epochs: Option<u32>,
    pub learning_rate: Option<f32>,
    pub batch: Option<u32>,
    pub seed: Option<u64>,
    pub windows_per_epoch: Option<u32>,
}

impl Default for TrainRequest {
    fn default() -> Self {
        TrainRequest {
            spec_id: "atom-mlp-tiny".into(),
            context: None,
            hidden: None,
            epochs: None,
            learning_rate: None,
            batch: None,
            seed: None,
            windows_per_epoch: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct DistillRequest {
    pub spec_id: String,
    pub prompts: u32,
    pub max_tokens: u32,
    pub epochs: Option<u32>,
}

impl Default for DistillRequest {
    fn default() -> Self {
        DistillRequest {
            spec_id: "atom-mlp-small".into(),
            prompts: 12,
            max_tokens: 96,
            epochs: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetRequest {
    pub path: String,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub val_ratio: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BenchmarkRequest {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InferRequest {
    pub prompt: String,
    #[serde(default = "default_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_tokens() -> u32 {
    120
}

fn default_temperature() -> f32 {
    0.7
}

#[derive(Clone, Debug, Serialize)]
pub struct InferResponse {
    pub output: String,
    pub tokens: u32,
    pub total_ms: u64,
    pub bytes_per_second: f64,
    pub model_path: String,
    pub params: u64,
    pub format: String,
}

fn selection_from(spec_id: &str, req: &TrainRequest) -> Result<ArchSelection> {
    let spec = architecture(spec_id)
        .ok_or_else(|| AtomError::Invalid(format!("unknown architecture '{spec_id}'")))?;
    if !spec.trainable_here {
        return Err(AtomError::Unavailable(format!(
            "{} is trained by the external adapter and requires {}. Install it with \
             `pip install torch`, or choose a built-in Atom MLP architecture.",
            spec.name,
            spec.requires.unwrap_or("an external toolchain")
        )));
    }
    let mut selection = ArchSelection::from_spec(&spec);
    if let Some(context) = req.context {
        selection.context = context.clamp(2, 32);
    }
    if let Some(hidden) = req.hidden {
        selection.hidden = hidden.clamp(4, 256);
    }
    if let Some(epochs) = req.epochs {
        selection.epochs = epochs.clamp(1, 200);
    }
    if let Some(lr) = req.learning_rate {
        selection.learning_rate = lr.clamp(0.0001, 1.0);
    }
    if let Some(batch) = req.batch {
        selection.batch = batch.clamp(1, 256);
    }
    if let Some(seed) = req.seed {
        selection.seed = seed;
    }
    if let Some(windows) = req.windows_per_epoch {
        selection.windows_per_epoch = windows.clamp(64, 200_000);
    }
    Ok(selection)
}

pub fn import_dataset(state: &AppState, project_id: &str, req: DatasetRequest) -> Result<MicroProject> {
    let mut project = state.inner.micro.get(project_id)?;
    let guard = state.guard();
    let source = guard.resolve_existing(&req.path)?;
    let dir = state.inner.micro.project_dir(project_id);
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "dataset".into());
    let imported = dataset::import(
        &source,
        &dir,
        &name,
        req.seed.unwrap_or(1337),
        req.val_ratio.unwrap_or(0.15),
    )?;
    project.dataset = Some(imported.info);
    state.inner.micro.save(&mut project)?;
    state.log(
        "info",
        "micro",
        format!("imported dataset for {} ({} documents)", project.name, project.dataset.as_ref().map(|d| d.stats.documents).unwrap_or(0)),
    );
    Ok(project)
}

impl AppState {
    pub fn start_micro_train(&self, project_id: &str, req: TrainRequest) -> Result<String> {
        let project = self.inner.micro.get(project_id)?;
        if project.dataset.is_none() {
            return Err(AtomError::Conflict(
                "import a dataset before training this project".into(),
            ));
        }
        let selection = selection_from(&req.spec_id, &req)?;
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Train,
            format!("Train {}", project.name),
            json!({ "project_id": project_id, "spec_id": selection.spec_id, "context": selection.context, "hidden": selection.hidden, "epochs": selection.epochs }),
        );
        let state = self.clone();
        let project_id = project_id.to_string();
        let run_selection = selection.clone();
        tokio::spawn(async move {
            job.start();
            job.progress("loading dataset", 0.01);
            let (train, val) = match state.inner.micro.read_dataset(&project_id) {
                Ok(data) => data,
                Err(e) => {
                    job.finish_err(e.to_string());
                    return;
                }
            };
            job.log(
                "info",
                format!(
                    "dataset: {} training bytes, {} validation bytes",
                    train.len(),
                    val.len()
                ),
            );
            let cfg = TrainConfig {
                epochs: run_selection.epochs,
                learning_rate: run_selection.learning_rate,
                batch: run_selection.batch,
                seed: run_selection.seed,
                windows_per_epoch: run_selection.windows_per_epoch,
            };
            let ctx = run_selection.context as usize;
            let hidden = run_selection.hidden as usize;
            let seed = run_selection.seed;
            let job_progress = job.clone();
            let job_log = job.clone();
            let job_cancel = job.clone();
            let started_at = chrono::Local::now().to_rfc3339();
            let task = tokio::task::spawn_blocking(move || {
                let mut model = ByteMlp::new(ctx, hidden, seed);
                let mut last_stage = String::new();
                let result = model.train(
                    &train,
                    &val,
                    &cfg,
                    |stage, fraction| {
                        if stage != last_stage {
                            job_log.log("train", stage);
                            last_stage = stage.to_string();
                        }
                        job_progress.progress(stage, 0.02 + fraction * 0.96);
                    },
                    || job_cancel.is_cancelled(),
                );
                (model, result)
            })
            .await;
            match task {
                Err(e) => job.finish_err(format!("training task failed: {e}")),
                Ok((_model, Err(e))) => {
                    if matches!(e, AtomError::Cancelled(_)) {
                        job.log("warn", "no model file was written");
                        job.finish_cancelled();
                    } else {
                        job.finish_err(e.to_string());
                    }
                }
                Ok((model, Ok(report))) => {
                    let run_id = job.id().to_string();
                    let path = state
                        .inner
                        .micro
                        .model_dir(&project_id)
                        .join(format!("{run_id}.atomlm"));
                    let saved = model.save(&path).and_then(|_| {
                        std::fs::metadata(&path).map(|m| m.len()).map_err(Into::into)
                    });
                    let bytes = match saved {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            job.finish_err(format!("could not write the trained model: {e}"));
                            return;
                        }
                    };
                    job.log(
                        "info",
                        format!(
                            "wrote {} ({} bytes, {:.4} val loss, {:.2} ppl)",
                            path.display(),
                            bytes,
                            report.val_loss,
                            report.perplexity
                        ),
                    );
                    let mut project = match state.inner.micro.get(&project_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.finish_err(e.to_string());
                            return;
                        }
                    };
                    project.architecture = Some(run_selection.clone());
                    project.training = Some(TrainingSummary {
                        run_id: run_id.clone(),
                        job_id: run_id.clone(),
                        model_path: path.display().to_string(),
                        model_bytes: bytes,
                        started_at,
                        finished_at: chrono::Local::now().to_rfc3339(),
                        report: report.clone(),
                    });
                    project.evaluation = Some(atom_micro::EvalReport {
                        windows: report.val_windows,
                        loss: report.val_loss,
                        perplexity: report.perplexity,
                        top1: report.top1,
                        top5: report.top5,
                        duration_ms: report.duration_ms,
                        samples: report.samples.clone(),
                        note: "Measured on the held-out validation split after training.".into(),
                    });
                    project.quantization = None;
                    project.export = None;
                    project.memory = None;
                    project.firmware = None;
                    if let Err(e) = state.inner.micro.save(&mut project) {
                        job.finish_err(e.to_string());
                        return;
                    }
                    job.finish_ok(json!({
                        "project_id": project_id,
                        "model_path": path.display().to_string(),
                        "run_id": run_id,
                        "report": report,
                    }));
                }
            }
        });
        Ok(job_id)
    }

    pub fn start_micro_firmware(&self, project_id: &str, upload: bool) -> Result<String> {
        let project = self.inner.micro.get(project_id)?;
        let target = hardware::target(&project.target_id)
            .ok_or_else(|| AtomError::Invalid("project target is unknown".into()))?;
        if target.platformio_board.is_none() {
            return Err(AtomError::Unavailable(format!(
                "{} is the reference target; select a microcontroller to build firmware",
                target.name
            )));
        }
        let export = project.export.as_ref().ok_or_else(|| {
            AtomError::Conflict("export the model before building firmware".into())
        })?;
        if project.training.is_none() {
            return Err(AtomError::Conflict("train a model before building firmware".into()));
        }
        if upload && !project.firmware.as_ref().map(|f| f.built).unwrap_or(false) {
            return Err(AtomError::Conflict(
                "build the firmware successfully before flashing it".into(),
            ));
        }
        if atom_micro::tools::find_program("pio").is_none() {
            return Err(AtomError::Unavailable(
                "PlatformIO Core (`pio`) for firmware builds and flashing".into(),
            ));
        }
        let (job_id, job) = self.inner.jobs.create(
            if upload { JobKind::Deploy } else { JobKind::Export },
            format!("{} {}", if upload { "Flash" } else { "Build firmware" }, project.name),
            json!({ "project_id": project_id, "target": target.id, "upload": upload }),
        );
        let state = self.clone();
        let project_id = project_id.to_string();
        let export_dir = std::path::PathBuf::from(&export.dir);
        let project_name = project.name.clone();
        let target_id = target.id.to_string();
        let board = target.platformio_board.unwrap_or_default().to_string();
        let dir = state.inner.micro.firmware_dir().join(&project_id);
        tokio::spawn(async move {
            job.start();
            job.progress("generating firmware project", 0.02);
            if let Err(e) = firmware::generate_project(&dir, &project_name, target, &export_dir) {
                job.finish_err(e.to_string());
                return;
            }
            job.log("info", format!("project written to {}", dir.display()));
            let job_log = job.clone();
            let job_cancel = job.clone();
            let dir_for_block = dir.clone();
            let task = tokio::task::spawn_blocking(move || {
                if upload {
                    firmware::upload(&dir_for_block, || job_cancel.is_cancelled(), |line| {
                        job_log.log("tool", line)
                    })
                    .map(|tail| (true, tail))
                } else {
                    firmware::build(&dir_for_block, || job_cancel.is_cancelled(), |line| {
                        job_log.log("tool", line)
                    })
                    .map(|report| (report.built, report.log_tail))
                }
            })
            .await;
            match task {
                Err(e) => job.finish_err(format!("tool task failed: {e}")),
                Ok(Err(e)) => {
                    if matches!(e, AtomError::Cancelled(_)) {
                        job.finish_cancelled();
                    } else {
                        job.finish_err(e.to_string());
                    }
                }
                Ok(Ok((success, tail))) => {
                    let artifact = firmware::find_firmware(&dir);
                    let mut project = match state.inner.micro.get(&project_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.finish_err(e.to_string());
                            return;
                        }
                    };
                    let mut report = project.firmware.clone().unwrap_or(atom_micro::FirmwareReport {
                        dir: dir.display().to_string(),
                        target_id: target_id.clone(),
                        board: board.clone(),
                        generated_at: chrono::Local::now().to_rfc3339(),
                        built: false,
                        build_tool: "platformio".into(),
                        artifact_path: None,
                        log_tail: Vec::new(),
                        error: None,
                        deployed: false,
                        deployed_at: None,
                    });
                    report.dir = dir.display().to_string();
                    report.target_id = target_id.clone();
                    report.board = board.clone();
                    report.built = upload || success;
                    report.artifact_path = artifact.clone();
                    report.log_tail = tail;
                    report.error = if success {
                        None
                    } else {
                        Some("the build did not produce a firmware artifact; see the log".into())
                    };
                    report.deployed = upload && success;
                    report.deployed_at = if upload && success {
                        Some(chrono::Local::now().to_rfc3339())
                    } else {
                        report.deployed_at.clone()
                    };
                    project.firmware = Some(report);
                    if let Err(e) = state.inner.micro.save(&mut project) {
                        job.finish_err(e.to_string());
                        return;
                    }
                    if !success {
                        job.finish_err("the firmware tool reported a failure");
                        return;
                    }
                    job.finish_ok(json!({
                        "built": !upload,
                        "deployed": upload,
                        "artifact": artifact,
                        "dir": dir.display().to_string(),
                    }));
                }
            }
        });
        Ok(job_id)
    }

    pub fn start_micro_distill(&self, project_id: &str, req: DistillRequest) -> Result<String> {
        let project = self.inner.micro.get(project_id)?;
        if project.dataset.is_none() {
            return Err(AtomError::Conflict(
                "import a dataset first; its prompts are what the teacher labels".into(),
            ));
        }
        let selection = selection_from(
            &req.spec_id,
            &TrainRequest {
                spec_id: req.spec_id.clone(),
                epochs: req.epochs,
                ..Default::default()
            },
        )?;
        let status = self.inner.runtime_status.read().map(|s| s.clone()).ok();
        let loaded = status.as_ref().map(|s| s.loaded).unwrap_or(false);
        if !loaded {
            return Err(AtomError::Unavailable(
                "Distillation needs a teacher loaded in the Inference workspace. Load a GGUF model, \
                 then run distillation; the teacher generates the corpus the student learns from."
                    .into(),
            ));
        }
        let teacher_label = status
            .as_ref()
            .and_then(|s| s.model_label.clone())
            .unwrap_or_else(|| "teacher".into());
        let teacher_id = status
            .as_ref()
            .and_then(|s| s.model_id.clone())
            .unwrap_or_default();
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Distillation,
            format!("Distill {} from {}", project.name, teacher_label),
            json!({ "project_id": project_id, "teacher": teacher_id, "spec_id": selection.spec_id }),
        );
        let state = self.clone();
        let project_id = project_id.to_string();
        let prompts_limit = req.prompts.clamp(1, 64) as usize;
        let max_tokens = req.max_tokens.clamp(16, 512);
        tokio::spawn(async move {
            job.start();
            let docs = match state.inner.micro.get(&project_id) {
                Ok(p) => match p.dataset {
                    Some(_) => {
                        let file = state.inner.micro.project_dir(&project_id).join("dataset.jsonl");
                        std::fs::read_to_string(&file).unwrap_or_default()
                    }
                    None => String::new(),
                },
                Err(e) => {
                    job.finish_err(e.to_string());
                    return;
                }
            };
            let prompts: Vec<String> = docs
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| l.len() > 12)
                .take(prompts_limit)
                .collect();
            if prompts.is_empty() {
                job.finish_err("the dataset has no usable prompt lines");
                return;
            }
            job.log("info", format!("labelling {} prompts with {}", prompts.len(), teacher_label));
            let started = std::time::Instant::now();
            let mut corpus: Vec<String> = Vec::new();
            let mut teacher_tokens = 0u64;
            for (index, prompt) in prompts.iter().enumerate() {
                if job.is_cancelled() {
                    job.finish_cancelled();
                    return;
                }
                job.progress(
                    &format!("teacher {}/{}", index + 1, prompts.len()),
                    0.02 + 0.33 * ((index + 1) as f64 / prompts.len() as f64),
                );
                let request = ChatRequest {
                    messages: vec![ChatMessage {
                        role: "user".into(),
                        content: prompt.clone(),
                    }],
                    max_tokens,
                    temperature: 0.7,
                    top_p: 0.95,
                    seed: Some(42),
                    stop: Vec::new(),
                };
                let guard = state.inner.runtime.lock().await;
                let Some(runtime) = guard.as_ref() else {
                    job.finish_err("the teacher was unloaded during distillation");
                    return;
                };
                let outcome = runtime.complete(&request).await;
                drop(guard);
                match outcome {
                    Ok(completion) => {
                        teacher_tokens += completion.completion_tokens;
                        corpus.push(format!("{}\n{}", prompt, completion.content.trim()));
                        job.log(
                            "teacher",
                            format!("prompt {} → {} tokens in {} ms", index + 1, completion.completion_tokens, completion.total_ms),
                        );
                    }
                    Err(e) => {
                        job.finish_err(format!("teacher call failed: {e}"));
                        return;
                    }
                }
            }
            let dir = state.inner.micro.project_dir(&project_id).join("distill");
            let info = match dataset::write_text_dataset(&dir, &corpus, &teacher_label) {
                Ok(info) => info,
                Err(e) => {
                    job.finish_err(e.to_string());
                    return;
                }
            };
            let (train, val) = match (
                std::fs::read(dir.join("dataset-distill.txt")),
                std::fs::read(dir.join("dataset-distill-val.txt")),
            ) {
                (Ok(t), Ok(v)) => (t, v),
                _ => {
                    job.finish_err("could not read the teacher corpus back from disk");
                    return;
                }
            };
            job.progress("training student", 0.38);
            let cfg = TrainConfig {
                epochs: selection.epochs,
                learning_rate: selection.learning_rate,
                batch: selection.batch,
                seed: selection.seed,
                windows_per_epoch: selection.windows_per_epoch,
            };
            let ctx = selection.context as usize;
            let hidden = selection.hidden as usize;
            let seed = selection.seed;
            let job_progress = job.clone();
            let job_cancel = job.clone();
            let inner_job = job.clone();
            let task = tokio::task::spawn_blocking(move || {
                let mut model = ByteMlp::new(ctx, hidden, seed);
                let report = model.train(
                    &train,
                    &val,
                    &cfg,
                    |stage, fraction| job_progress.progress(stage, 0.38 + fraction * 0.55),
                    || job_cancel.is_cancelled(),
                )?;
                let agreement = atom_micro::teacher_agreement(&model, &train, 1200);
                let corpus_len = train.len() + val.len();
                let _ = inner_job;
                Ok::<_, AtomError>((model, report, agreement, corpus_len))
            })
            .await;
            match task {
                Err(e) => job.finish_err(format!("distillation task failed: {e}")),
                Ok(Err(e)) => {
                    if matches!(e, AtomError::Cancelled(_)) {
                        job.finish_cancelled();
                    } else {
                        job.finish_err(e.to_string());
                    }
                }
                Ok(Ok((model, report, agreement, _corpus_len))) => {
                    let run_id = job.id().to_string();
                    let path = state
                        .inner
                        .micro
                        .model_dir(&project_id)
                        .join(format!("distill-{run_id}.atomlm"));
                    if let Err(e) = model.save(&path) {
                        job.finish_err(e.to_string());
                        return;
                    }
                    let mut project = match state.inner.micro.get(&project_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.finish_err(e.to_string());
                            return;
                        }
                    };
                    project.architecture = Some(selection.clone());
                    project.training = Some(TrainingSummary {
                        run_id: run_id.clone(),
                        job_id: run_id.clone(),
                        model_path: path.display().to_string(),
                        model_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                        started_at: chrono::Local::now().to_rfc3339(),
                        finished_at: chrono::Local::now().to_rfc3339(),
                        report: report.clone(),
                    });
                    project.distillation = Some(DistillReport {
                        teacher_model_id: teacher_id.clone(),
                        teacher_label: teacher_label.clone(),
                        prompts: prompts.len(),
                        teacher_tokens,
                        duration_ms: started.elapsed().as_millis() as u64,
                        agreement,
                        report: report.clone(),
                        dataset: info,
                        model_path: path.display().to_string(),
                        created_at: chrono::Local::now().to_rfc3339(),
                        method: "Teacher completions form the student corpus; the student is trained \
                                 on that corpus and agreement is measured teacher-forced on the same text."
                            .into(),
                    });
                    project.quantization = None;
                    project.export = None;
                    project.memory = None;
                    project.firmware = None;
                    if let Err(e) = state.inner.micro.save(&mut project) {
                        job.finish_err(e.to_string());
                        return;
                    }
                    job.finish_ok(json!({
                        "project_id": project_id,
                        "agreement": agreement,
                        "prompts": prompts.len(),
                        "teacher_tokens": teacher_tokens,
                        "val_perplexity": report.perplexity,
                        "model_path": path.display().to_string(),
                    }));
                }
            }
        });
        Ok(job_id)
    }
}

pub fn evaluate_project(state: &AppState, project_id: &str) -> Result<atom_micro::EvalReport> {
    let mut project = state.inner.micro.get(project_id)?;
    let training = project
        .training
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("train a model before evaluating it".into()))?;
    let model = ByteMlp::load(std::path::Path::new(&training.model_path))?;
    let (_, val) = state.inner.micro.read_dataset(project_id)?;
    let report = model.evaluate(&val, 2000);
    project.evaluation = Some(report.clone());
    state.inner.micro.save(&mut project)?;
    Ok(report)
}

pub fn quantize_project(state: &AppState, project_id: &str, format: &str) -> Result<QuantSummary> {
    let mut project = state.inner.micro.get(project_id)?;
    let training = project
        .training
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("train a model before quantizing it".into()))?
        .clone();
    let (_, val) = state.inner.micro.read_dataset(project_id)?;
    let model = ByteMlp::load(std::path::Path::new(&training.model_path))?;
    let started = std::time::Instant::now();
    let quantized = model.quantize(format)?;
    let base = model.evaluate(&val, 1200);
    let restored = quantized.dequantized().evaluate(&val, 1200);
    let path = state
        .inner
        .micro
        .model_dir(project_id)
        .join(format!("{}.atomq", format));
    quantized.write_container(&path)?;
    let bytes = std::fs::metadata(&path)?.len();
    let report = atom_micro::QuantReport {
        format: format.to_string(),
        params: quantized.params(),
        weight_bytes: quantized.weight_bytes,
        f32_bytes: quantized.f32_bytes(),
        ratio: quantized.f32_bytes() as f64 / quantized.weight_bytes.max(1) as f64,
        bits_per_weight: quantized.weight_bytes as f64 * 8.0 / quantized.params().max(1) as f64,
        val_loss: restored.loss,
        perplexity: restored.perplexity,
        top1: restored.top1,
        delta_loss: restored.loss - base.loss,
        duration_ms: started.elapsed().as_millis() as u64,
        note: "Quantization is symmetric per output row. The loss delta is measured by \
               dequantizing the weights and re-running validation on the same split."
            .into(),
    };
    let target = hardware::target(&project.target_id)
        .ok_or_else(|| AtomError::Invalid("project target is unknown".into()))?;
    let plan = hardware::plan(
        target,
        format,
        quantized.params(),
        quantized.weight_bytes,
        quantized.context as u32,
        quantized.hidden as u32,
    );
    let summary = QuantSummary {
        format: format.to_string(),
        path: path.display().to_string(),
        bytes,
        report: report.clone(),
        created_at: chrono::Local::now().to_rfc3339(),
    };
    project.quantization = Some(summary.clone());
    project.memory = Some(plan);
    project.export = None;
    project.firmware = None;
    state.inner.micro.save(&mut project)?;
    Ok(summary)
}

pub fn plan_project(
    state: &AppState,
    project_id: &str,
    target_override: Option<&str>,
) -> Result<hardware::MemoryPlan> {
    let mut project = state.inner.micro.get(project_id)?;
    let training = project
        .training
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("train a model before analysing memory".into()))?
        .clone();
    let quant = project
        .quantization
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("quantize the model before analysing memory".into()))?
        .clone();
    let target = hardware::target(target_override.unwrap_or(&project.target_id))
        .ok_or_else(|| AtomError::Invalid("project target is unknown".into()))?;
    let plan = hardware::plan(
        target,
        &quant.format,
        quant.report.params,
        quant.report.weight_bytes,
        training.report.context,
        training.report.hidden,
    );
    if target_override.is_none() || target_override == Some(project.target_id.as_str()) {
        project.memory = Some(plan.clone());
        state.inner.micro.save(&mut project)?;
    }
    Ok(plan)
}

pub fn export_project(state: &AppState, project_id: &str) -> Result<atom_micro::ExportReport> {
    let mut project = state.inner.micro.get(project_id)?;
    let training = project
        .training
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("train a model before exporting it".into()))?
        .clone();
    let quant = project
        .quantization
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("quantize the model before exporting it".into()))?
        .clone();
    let target = hardware::target(&project.target_id)
        .ok_or_else(|| AtomError::Invalid("project target is unknown".into()))?;
    let model = ByteMlp::load(std::path::Path::new(&training.model_path))?;
    let quantized = model.quantize(&quant.format)?;
    let memory = project.memory.clone();
    let dir = state
        .inner
        .micro
        .exports_dir()
        .join(project_id)
        .join(export::export_dir_name(&project.name, &quant.format));
    let report = export::export_bundle(
        &quantized,
        &dir,
        &ExportInput {
            project_name: &project.name,
            architecture: project
                .architecture
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or("ATOM Micro model"),
            dataset_documents: project.dataset.as_ref().map(|d| d.stats.documents),
            validation_perplexity: Some(training.report.perplexity),
            target,
            memory: memory.as_ref(),
            quantization_report: serde_json::to_value(&quant.report).ok(),
        },
    )?;
    project.export = Some(report.clone());
    project.firmware = None;
    state.inner.micro.save(&mut project)?;
    Ok(report)
}

pub fn infer_project(state: &AppState, project_id: &str, req: InferRequest) -> Result<InferResponse> {
    let project = state.inner.micro.get(project_id)?;
    let training = project
        .training
        .as_ref()
        .ok_or_else(|| AtomError::Conflict("train a model before running inference".into()))?;
    let model = ByteMlp::load(std::path::Path::new(&training.model_path))?;
    let started = std::time::Instant::now();
    let output = model.generate(req.prompt.as_bytes(), req.max_tokens as usize, req.temperature, 12345);
    let elapsed = started.elapsed();
    let bytes = output.len() as u32;
    Ok(InferResponse {
        output,
        tokens: req.max_tokens,
        total_ms: elapsed.as_millis() as u64,
        bytes_per_second: bytes as f64 / elapsed.as_secs_f64().max(1e-6),
        model_path: training.model_path.clone(),
        params: training.report.params,
        format: project
            .quantization
            .as_ref()
            .map(|q| q.format.clone())
            .unwrap_or_else(|| "f32".into()),
    })
}

pub fn import_benchmarks(
    state: &AppState,
    project_id: &str,
    req: BenchmarkRequest,
) -> Result<Vec<atom_micro::DeviceBenchmark>> {
    let mut project = state.inner.micro.get(project_id)?;
    let guard = state.guard();
    let path = guard.resolve_existing(&req.path)?;
    let (_, _, runs) = atom_micro::parse_report(&path, &path)?;
    project.benchmarks.extend(runs.clone());
    state.inner.micro.save(&mut project)?;
    Ok(runs)
}

pub fn overview(state: &AppState) -> serde_json::Value {
    let projects = state.inner.micro.list();
    let storage = dir_bytes(&state.inner.micro.root());
    let trained = projects.iter().filter(|p| p.params.is_some()).count();
    let quantized = projects.iter().filter(|p| p.quant_format.is_some()).count();
    let deployed = projects.iter().filter(|p| p.firmware_built).count();
    json!({
        "projects": projects,
        "counts": {
            "projects": projects.len(),
            "trained": trained,
            "quantized": quantized,
            "built": deployed,
        },
        "storage_bytes": storage,
        "started_at": state.inner.started_at.to_rfc3339(),
    })
}

fn dir_bytes(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            total += dir_bytes(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

