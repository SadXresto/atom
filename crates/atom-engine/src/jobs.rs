//! Long-running jobs: import, verification, quantization and benchmarking.

use atom_bench::{
    all_cases, score_output, Environment, ModelRun, RunSettings, SuiteResult, ConsistencyResult, CaseResult,
};
use atom_compression::{build_plan, default_output_name, validate_output};
use atom_core::{AtomError, JobKind, Result};
use atom_inference::{ChatMessage, ChatRequest, LoadOptions};
use atom_models::{import_model, verify_record, ModelKind, ModelRecord, Verification};
use atom_security::validate_file_name;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::state::AppState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportRequest {
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizeRequest {
    pub model_id: String,
    pub format: String,
    /// File name only; the directory is always the ATOM compressed-models dir.
    pub output_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComparisonRequest {
    pub model_ids: Vec<String>,
    pub suite_ids: Vec<String>,
    pub settings: Option<RunSettings>,
    pub title: Option<String>,
}

impl AppState {
    pub fn start_import(&self, req: ImportRequest) -> Result<String> {
        let state = self.clone();
        let cfg = self.config();
        let guard = self.guard();
        let source = PathBuf::from(&req.path);
        let models_dir = PathBuf::from(&cfg.models_dir);
        let verify = cfg.verify_checksums;
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Import,
            format!("Import {}", source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            serde_json::json!({ "path": req.path }),
        );
        tokio::spawn(async move {
            job.start();
            job.progress("copying model into the library", 0.01);
            let job_for_block = job.clone();
            let result = tokio::task::spawn_blocking(move || {
                import_model(&guard, &models_dir, &source, verify, |done, total| {
                    let fraction = if total > 0 { (done as f64 / total as f64) * 0.85 } else { 0.05 };
                    job_for_block.progress("copying model into the library", fraction.max(0.01));
                })
            })
            .await;

            match result {
                Ok(Ok(outcome)) => {
                    let record = outcome.record;
                    job.progress("indexing model", 0.95);
                    if let Ok(mut lib) = state.inner.library.write() {
                        if let Err(e) = lib.insert(record.clone()) {
                            job.finish_err(e.to_string());
                            return;
                        }
                    }
                    job.log(
                        "info",
                        format!(
                            "imported {} ({} bytes, {} tensors, {})",
                            record.file_name,
                            record.size_bytes,
                            record.summary.tensor_count,
                            record.summary.label_quant()
                        ),
                    );
                    state.log("info", "import", format!("imported {}", record.file_name));
                    job.finish_ok(serde_json::json!({ "model_id": record.id, "model": record }));
                }
                Ok(Err(e)) => job.finish_err(e.to_string()),
                Err(e) => job.finish_err(format!("import task failed: {e}")),
            }
        });
        Ok(job_id)
    }

    pub fn start_verify(&self, model_id: &str) -> Result<String> {
        let state = self.clone();
        let record = self
            .model(model_id)
            .ok_or_else(|| AtomError::NotFound(format!("model {model_id} is not in the library")))?;
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Verification,
            format!("Verify {}", record.label),
            serde_json::json!({ "model_id": model_id }),
        );
        tokio::spawn(async move {
            job.start();
            job.progress("hashing model file", 0.1);
            let mut record = record;
            let result = tokio::task::spawn_blocking({
                let mut rec = record.clone();
                move || {
                    verify_record(&mut rec)?;
                    Ok::<_, AtomError>(rec)
                }
            })
            .await;
            match result {
                Ok(Ok(updated)) => {
                    let failed = matches!(updated.verification, Verification::Failed { .. });
                    if let Ok(mut lib) = state.inner.library.write() {
                        if let Some(slot) = lib.get_mut(&updated.id) {
                            *slot = updated.clone();
                        }
                        let _ = lib.save();
                    }
                    record.verification = updated.verification.clone();
                    job.log("info", format!("verification state: {:?}", updated.verification));
                    if failed {
                        job.finish_err("checksum verification failed");
                    } else {
                        job.finish_ok(serde_json::json!({ "verification": updated.verification }));
                    }
                }
                Ok(Err(e)) => job.finish_err(e.to_string()),
                Err(e) => job.finish_err(format!("verification task failed: {e}")),
            }
        });
        Ok(job_id)
    }

    pub fn start_quantize(&self, req: QuantizeRequest) -> Result<String> {
        if self.inner.jobs.active_of_kind(JobKind::Quantize).is_some() {
            return Err(AtomError::Conflict(
                "another quantization job is already running; wait for it or cancel it".into(),
            ));
        }
        let state = self.clone();
        let record = self
            .model(&req.model_id)
            .ok_or_else(|| AtomError::NotFound(format!("model {} is not in the library", req.model_id)))?;
        if let ModelKind::Compressed { .. } = record.kind {
            // Allowed: re-compressing a variant is a legitimate workflow.
        }
        let tool = self.tool();
        if !tool.available {
            return Err(AtomError::Unavailable(tool.message));
        }
        let cfg = self.config();
        let output_name = match req.output_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(name) => {
                validate_file_name(name)?;
                if !name.to_lowercase().ends_with(".gguf") {
                    format!("{name}.gguf")
                } else {
                    name.to_string()
                }
            }
            None => default_output_name(&record.file_name, &req.format),
        };
        let guard = self.guard();
        let outputs_dir = PathBuf::from(&cfg.outputs_dir);
        let output_path = guard.resolve_new(outputs_dir.join(&output_name))?;
        if record.path == output_path.display().to_string() {
            return Err(AtomError::Invalid("output path would overwrite the source model".into()));
        }
        let plan = build_plan(
            &tool,
            std::path::Path::new(&record.path),
            &output_path,
            &req.format,
            cfg.threads,
            record.summary.requires_requantize(),
        )?;
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Quantize,
            format!("{} → {}", record.label, req.format),
            serde_json::json!({
                "model_id": record.id,
                "format": req.format,
                "output": output_path.display().to_string()
            }),
        );
        let task_job_id = job_id.clone();
        tokio::spawn(async move {
            job.start();
            job.log("info", format!("source: {}", record.path));
            job.log("info", format!("output: {}", output_path.display()));
            let source_path = PathBuf::from(&record.path);
            let source_tensor_count = record.summary.tensor_count;
            let source_size = record.size_bytes;
            let format = req.format.clone();
            let run = atom_compression::run_quantize(&job, &plan).await;
            let outcome = match run {
                Ok(o) => o,
                Err(_) if job.is_cancelled() => {
                    job.finish_cancelled();
                    return;
                }
                Err(e) => {
                    job.finish_err(e.to_string());
                    state.log("error", "quantize", e.to_string());
                    return;
                }
            };
            job.log(
                "info",
                format!(
                    "llama-quantize finished in {} ms, wrote {} bytes",
                    outcome.duration_ms, outcome.produced_bytes
                ),
            );

            // Write the run log next to the output for the record.
            let log_path = output_path.with_extension("log");
            let lines = state.inner.jobs.logs(&task_job_id, 0);
            let mut log_text = String::new();
            for line in &lines {
                log_text.push_str(&format!("[{}] {} {}\n", line.ts, line.stream, line.message));
            }
            let _ = std::fs::write(&log_path, log_text);

            job.progress("validating output", 0.98);
            let validation = match validate_output(&output_path, &source_path, source_tensor_count, &format) {
                Ok(v) => v,
                Err(e) => {
                    let _ = std::fs::remove_file(&output_path);
                    let _ = std::fs::remove_file(&log_path);
                    job.finish_err(format!("validation failed: {e}"));
                    return;
                }
            };
            for check in &validation.checks {
                job.log(
                    if check.passed { "info" } else { "error" },
                    format!("[{}] {}: {}", check.severity, check.name, check.detail),
                );
            }
            if !validation.ok {
                let _ = std::fs::remove_file(&output_path);
                let _ = std::fs::remove_file(&log_path);
                job.finish_err(
                    "the produced file failed validation; it was removed rather than added to the library"
                        .to_string(),
                );
                return;
            }

            let output_bytes = validation.output_bytes;
            let output_sha = validation.output_sha256.clone().unwrap_or_default();
            let output_gguf = match atom_models::GgufFile::open(&output_path) {
                Ok(g) => g,
                Err(e) => {
                    job.finish_err(format!("output could not be re-read: {e}"));
                    return;
                }
            };
            let new_record = ModelRecord {
                id: uuid::Uuid::new_v4().to_string(),
                label: format!("{} · {}", record.label, format),
                file_name: output_name.clone(),
                path: output_path.display().to_string(),
                size_bytes: output_bytes,
                sha256: validation.output_sha256.clone(),
                verification: Verification::Verified { checked_at: chrono::Local::now().to_rfc3339() },
                imported_at: chrono::Local::now().to_rfc3339(),
                kind: ModelKind::Compressed {
                    source_model_id: record.id.clone(),
                    source_sha256: record.sha256.clone(),
                    format: format.clone(),
                    job_id: task_job_id.clone(),
                    output_sha256: output_sha,
                    source_size_bytes: source_size,
                },
                summary: atom_models::ModelSummary::from_gguf(&output_gguf, 0),
                notes: format!("Produced by llama-quantize in job {task_job_id}"),
            };
            if let Ok(mut lib) = state.inner.library.write() {
                if let Err(e) = lib.insert(new_record.clone()) {
                    job.finish_err(e.to_string());
                    return;
                }
            }
            state.log(
                "info",
                "quantize",
                format!("{} produced {} ({:.1}% smaller)", format, output_name, validation.savings_percent),
            );
            job.progress("done", 1.0);
            job.finish_ok(serde_json::json!({
                "model_id": new_record.id,
                "model": new_record,
                "validation": validation,
                "duration_ms": outcome.duration_ms,
                "log_path": log_path.display().to_string(),
            }));
        });
        Ok(job_id)
    }

    pub fn start_comparison(&self, req: ComparisonRequest) -> Result<String> {
        if req.model_ids.is_empty() {
            return Err(AtomError::Invalid("select at least one model".into()));
        }
        let backend = self.backend();
        if !backend.inference_ready {
            return Err(AtomError::Unavailable(backend.message));
        }
        let mut models = Vec::new();
        for id in &req.model_ids {
            let record = self
                .model(id)
                .ok_or_else(|| AtomError::NotFound(format!("model {id} is not in the library")))?;
            models.push(record);
        }
        let suite_ids: Vec<String> = if req.suite_ids.is_empty() {
            atom_bench::suites().iter().map(|s| s.id.to_string()).collect()
        } else {
            req.suite_ids.clone()
        };
        for id in &suite_ids {
            if !atom_bench::suites().iter().any(|s| s.id == id) {
                return Err(AtomError::Invalid(format!("unknown suite '{id}'")));
            }
        }
        let settings = req.settings.clone().unwrap_or_default();
        let title = req.title.clone().unwrap_or_else(|| {
            models.iter().map(|m| m.summary.label_quant()).collect::<Vec<_>>().join(" vs ")
        });
        let (job_id, job) = self.inner.jobs.create(
            JobKind::Benchmark,
            format!("Comparison: {title}"),
            serde_json::json!({ "model_ids": req.model_ids, "suite_ids": suite_ids }),
        );
        let state = self.clone();
        let task_job_id = job_id.clone();
        tokio::spawn(async move {
            job.start();
            let result =
                run_comparison_inner(&state, &job, models, &suite_ids, settings, title, &task_job_id).await;
            match result {
                Ok(cmp) => {
                    job.finish_ok(serde_json::json!({ "comparison_id": cmp.id }));
                    state.log("info", "bench", format!("comparison {} finished", cmp.id));
                }
                Err(_) if job.is_cancelled() => job.finish_cancelled(),
                Err(e) => {
                    job.finish_err(e.to_string());
                    state.log("error", "bench", e.to_string());
                }
            }
        });
        Ok(job_id)
    }
}

async fn run_comparison_inner(
    state: &AppState,
    job: &atom_core::JobHandle,
    models: Vec<ModelRecord>,
    suite_ids: &[String],
    settings: RunSettings,
    title: String,
    job_id: &str,
) -> Result<atom_bench::Comparison> {
    let backend = state.backend();
    let server_path = backend
        .server
        .as_ref()
        .map(|s| s.path.clone())
        .ok_or_else(|| AtomError::Unavailable(backend.message.clone()))?;
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|c| suite_ids.iter().any(|s| s == c.suite))
        .collect();
    let repeat_count = if suite_ids.iter().any(|s| s == "consistency") {
        settings.repeats.max(2)
    } else {
        0
    };
    let total_units = models.len() * (cases.len() + repeat_count as usize);
    let mut done_units = 0usize;
    let mut runs: Vec<ModelRun> = Vec::new();

    for record in &models {
        if job.is_cancelled() {
            return Err(job.cancelled_error());
        }
        job.progress(&format!("loading {}", record.label), done_units as f64 / total_units as f64);
        let options = LoadOptions {
            model_path: record.path.clone(),
            model_label: record.label.clone(),
            context_length: settings.context_length,
            threads: settings.threads,
            batch_size: settings.batch_size,
            gpu_layers: settings.gpu_layers,
            use_mmap: settings.use_mmap,
            use_mlock: false,
            flash_attention: state.config().flash_attention,
            kv_cache_type_k: settings.kv_cache_type_k.clone(),
            kv_cache_type_v: settings.kv_cache_type_v.clone(),
            memory_budget_mb: None,
        };
        let runtime = match atom_inference::Runtime::spawn(&server_path, options, |line| {
            job.log("info", line);
        })
        .await
        {
            Ok(rt) => rt,
            Err(e) => {
                job.log("error", format!("could not load {}: {e}", record.label));
                let mut failed = empty_run(record);
                failed.error = Some(e.to_string());
                runs.push(failed);
                done_units += cases.len() + repeat_count as usize;
                continue;
            }
        };
        let pid = runtime.info.pid;
        job.log("info", format!("{} loaded in {} ms (pid {pid})", record.label, runtime.info.load_ms));

        // Peak RSS sampler for this model's run.
        let peak = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let sampler = {
            let monitor = state.inner.monitor.clone();
            let peak = peak.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let rss = monitor.lock().await.sample(Some(pid)).model_process_rss.unwrap_or(0);
                    peak.fetch_max(rss, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            })
        };
        let rss_after_load = state
            .inner
            .monitor
            .lock()
            .await
            .sample(Some(pid))
            .model_process_rss
            .unwrap_or(0);

        let mut run = ModelRun {
            model_id: record.id.clone(),
            label: record.label.clone(),
            quant: record.summary.label_quant(),
            kind: record.display_kind().to_string(),
            file_size: record.size_bytes,
            load_ms: runtime.info.load_ms,
            peak_rss_bytes: 0,
            rss_after_load_bytes: rss_after_load,
            avg_ttft_ms: 0.0,
            avg_tokens_per_second: 0.0,
            total_generation_ms: 0,
            suites: Vec::new(),
            cases: Vec::new(),
            consistency: None,
            quality_index: 0.0,
            error: None,
        };

        for case in &cases {
            if job.is_cancelled() {
                stop.store(true, Ordering::SeqCst);
                let _ = sampler.await;
                let _ = runtime.unload().await;
                return Err(job.cancelled_error());
            }
            job.progress(
                &format!("{}: {}", record.label, case.title),
                done_units as f64 / total_units as f64,
            );
            let request = ChatRequest {
                messages: vec![ChatMessage { role: "user".into(), content: case.prompt.to_string() }],
                max_tokens: settings.max_tokens,
                temperature: settings.temperature,
                top_p: settings.top_p,
                seed: settings.seed,
                stop: Vec::new(),
            };
            let started = Instant::now();
            let outcome = runtime.complete(&request).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let mut result = CaseResult {
                case_id: case.id.to_string(),
                suite: case.suite.to_string(),
                title: case.title.to_string(),
                prompt: case.prompt.to_string(),
                output: String::new(),
                passed: false,
                detail: String::new(),
                latency_ms,
                tokens_per_second: 0.0,
                completion_tokens: 0,
                error: None,
            };
            match outcome {
                Ok(completion) => {
                    let score = score_output(&case.scoring, &completion.content);
                    result.output = completion.content;
                    result.passed = score.passed;
                    result.detail = score.detail.clone();
                    result.tokens_per_second = completion.tokens_per_second;
                    result.completion_tokens = completion.completion_tokens;
                    job.log(
                        "info",
                        format!(
                            "{} [{}] {:.1} tok/s in {} ms → {}",
                            case.id,
                            if score.passed { "pass" } else { "fail" },
                            completion.tokens_per_second,
                            completion.total_ms,
                            score.detail
                        ),
                    );
                }
                Err(e) => {
                    result.error = Some(e.to_string());
                    result.detail = e.to_string();
                    job.log("error", format!("{} failed: {e}", case.id));
                }
            }
            run.cases.push(result);
            done_units += 1;
        }

        if repeat_count > 0 {
            let mut samples = Vec::new();
            for i in 0..repeat_count {
                if job.is_cancelled() {
                    stop.store(true, Ordering::SeqCst);
                    let _ = sampler.await;
                    let _ = runtime.unload().await;
                    return Err(job.cancelled_error());
                }
                job.progress(
                    &format!("{}: consistency sample {}/{}", record.label, i + 1, repeat_count),
                    done_units as f64 / total_units as f64,
                );
                let request = ChatRequest {
                    messages: vec![ChatMessage {
                        role: "user".into(),
                        content: atom_bench::suites::CONSISTENCY_PROMPT.to_string(),
                    }],
                    max_tokens: 24,
                    temperature: settings.temperature,
                    top_p: settings.top_p,
                    seed: settings.seed.map(|s| s + i as i64),
                    stop: Vec::new(),
                };
                match runtime.complete(&request).await {
                    Ok(c) => samples.push(c.content.trim().to_string()),
                    Err(e) => job.log("error", format!("consistency sample failed: {e}")),
                }
                done_units += 1;
            }
            if !samples.is_empty() {
                let agreement = atom_bench::score::agreement(&samples);
                run.consistency = Some(ConsistencyResult {
                    prompt: atom_bench::suites::CONSISTENCY_PROMPT.to_string(),
                    detail: format!(
                        "{}/{} samples agreed on the same normalized answer",
                        (agreement * samples.len() as f64).round(),
                        samples.len()
                    ),
                    samples,
                    agreement,
                });
            }
        }

        stop.store(true, Ordering::SeqCst);
        let _ = sampler.await;
        run.peak_rss_bytes = peak.load(Ordering::SeqCst).max(rss_after_load);
        let unload_message = runtime.unload().await?;
        job.log("info", unload_message);

        // Aggregate suites.
        for suite in atom_bench::suites() {
            if !suite_ids.iter().any(|s| s == suite.id) || suite.kind != "quality" {
                continue;
            }
            let suite_cases: Vec<&CaseResult> = run.cases.iter().filter(|c| c.suite == suite.id).collect();
            if suite_cases.is_empty() {
                continue;
            }
            let passed = suite_cases.iter().filter(|c| c.passed).count();
            run.suites.push(SuiteResult {
                suite: suite.id.to_string(),
                title: suite.title.to_string(),
                passed,
                total: suite_cases.len(),
                pass_rate: passed as f64 / suite_cases.len() as f64,
                kind: "quality".into(),
            });
        }
        run.recompute_summary();
        job.log(
            "info",
            format!(
                "{}: quality {:.0}/100, {:.1} tok/s avg, peak RSS {:.1} MiB",
                record.label,
                run.quality_index,
                run.avg_tokens_per_second,
                run.peak_rss_bytes as f64 / 1048576.0
            ),
        );
        runs.push(run);
    }

    let env = Environment {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        total_memory: {
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            sys.total_memory()
        },
        atom_version: env!("CARGO_PKG_VERSION").to_string(),
        backend_version: state.backend().server.as_ref().and_then(|s| s.version.clone()),
        started_at: chrono::Local::now().to_rfc3339(),
    };
    let cmp = atom_bench::Comparison {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        title,
        settings,
        environment: env,
        runs,
        job_id: Some(job_id.to_string()),
        notes: String::new(),
    };
    state.inner.comparisons.save(&cmp)?;
    Ok(cmp)
}

fn empty_run(record: &ModelRecord) -> ModelRun {
    ModelRun {
        model_id: record.id.clone(),
        label: record.label.clone(),
        quant: record.summary.label_quant(),
        kind: record.display_kind().to_string(),
        file_size: record.size_bytes,
        load_ms: 0,
        peak_rss_bytes: 0,
        rss_after_load_bytes: 0,
        avg_ttft_ms: 0.0,
        avg_tokens_per_second: 0.0,
        total_generation_ms: 0,
        suites: Vec::new(),
        cases: Vec::new(),
        consistency: None,
        quality_index: 0.0,
        error: Some("model could not be loaded".into()),
    }
}

