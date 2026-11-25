use atom_core::{JobStatus, Paths};
use atom_engine::AppState;
use clap::{Parser, Subcommand};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "atom", about = "ATOM: local LLM optimization and benchmarking", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show toolchain, library and runtime status.
    Status,
    /// Inspect a GGUF file without importing it.
    Inspect { path: String },
    /// List library models.
    Models,
    /// Import a GGUF file into the library.
    Import {
        path: String,
        #[arg(long)]
        no_verify: bool,
    },
    /// Permanently quantize a library model.
    Quantize {
        model_id: String,
        format: String,
        #[arg(long)]
        output_name: Option<String>,
    },
    /// Show available quantization formats for a model.
    Formats { model_id: String },
}

fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let state = AppState::new(Paths::resolve())?;
    state.spawn_sampler();

    match cli.command {
        Commands::Status => {
            let backend = state.backend();
            println!("ATOM {}", env!("CARGO_PKG_VERSION"));
            println!("data root: {}", state.paths().root.display());
            println!("llama-server:  {}", backend.server.as_ref().map(|t| t.path.clone()).unwrap_or_else(|| "not found".into()));
            println!("llama-quantize: {}", backend.quantize.as_ref().map(|t| t.path.clone()).unwrap_or_else(|| "not found".into()));
            println!("inference ready: {}", backend.inference_ready);
            println!("compression ready: {}", backend.compression_ready);
            println!("models: {}", state.models().len());
            let runtime = state.runtime_status().await;
            println!(
                "loaded model: {}",
                runtime.model_label.unwrap_or_else(|| "none".into())
            );
        }
        Commands::Inspect { path } => {
            let gguf = atom_models::GgufFile::open(std::path::Path::new(&path))?;
            let summary = atom_models::ModelSummary::from_gguf(&gguf, 0);
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Commands::Models => {
            let models = state.models();
            if models.is_empty() {
                println!("library is empty");
            }
            for model in models {
                println!(
                    "{:<38} {:<10} {:<12} {:>10}  {}",
                    model.id,
                    model.summary.label_quant(),
                    model.display_kind(),
                    fmt_bytes(model.size_bytes),
                    model.label
                );
            }
        }
        Commands::Formats { model_id } => {
            let model = state
                .model(&model_id)
                .ok_or("model not in the library")?;
            let tool = state.tool();
            for format in atom_compression::formats_for(&tool, &model.summary, model.size_bytes) {
                println!(
                    "{:<8} {:<10} {:>10}  {}",
                    format.name,
                    if format.supported_for_source { "available" } else { "unavailable" },
                    fmt_bytes(format.estimated_bytes),
                    format.note
                );
            }
        }
        Commands::Import { path, no_verify } => {
            if no_verify {
                println!("note: checksums are still recorded on import; use Settings to disable verification");
            }
            let job_id = state.start_import(atom_engine::jobs::ImportRequest { path })?;
            wait_for_job(&state, &job_id).await?;
        }
        Commands::Quantize {
            model_id,
            format,
            output_name,
        } => {
            let job_id = state.start_quantize(atom_engine::jobs::QuantizeRequest {
                model_id,
                format,
                output_name,
            })?;
            println!("job {job_id} started");
            wait_for_job(&state, &job_id).await?;
        }
    }
    Ok(())
}

async fn wait_for_job(state: &AppState, job_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut seen = 0u64;
    let mut last_progress = -1.0f64;
    loop {
        for log in state.inner.jobs.logs(job_id, seen) {
            seen = log.seq;
            println!("[{}] {}", log.stream, log.message);
        }
        if let Some(job) = state.inner.jobs.get(job_id) {
            if (job.progress - last_progress).abs() > 0.02 {
                last_progress = job.progress;
                eprintln!("  {} {:.0}%", job.stage, job.progress * 100.0);
            }
            if job.status.is_terminal() {
                match job.status {
                    JobStatus::Succeeded => {
                        if let Some(result) = job.result {
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        }
                        return Ok(());
                    }
                    other => {
                        return Err(format!(
                            "job ended as {:?}: {}",
                            other,
                            job.error.unwrap_or_default()
                        )
                        .into())
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
