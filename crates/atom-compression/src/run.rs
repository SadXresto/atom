//! Run llama-quantize as a supervised child process.

use atom_core::{AtomError, JobHandle, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::plan::QuantizePlan;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizeOutcome {
    pub output_path: String,
    pub format: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub produced_bytes: u64,
}

/// Parse a llama-quantize tensor progress line:
/// `[  12/291]   blk.0.attn_q.weight - [ 4096, 4096, 1, 1], type = f16, converting to q4_K .. size = ...`
pub fn parse_progress(line: &str) -> Option<(u64, u64)> {
    let start = line.find('[')?;
    let rest = &line[start + 1..];
    let end = rest.find(']')?;
    let inner = rest[..end].trim();
    let mut parts = inner.split('/');
    let done = parts.next()?.trim().parse::<u64>().ok()?;
    let total = parts.next()?.trim().parse::<u64>().ok()?;
    if total == 0 || done > total {
        return None;
    }
    Some((done, total))
}

/// Run the plan. On cancellation or failure the partial output is deleted so an
/// incomplete file can never be mistaken for a usable model.
pub async fn run_quantize(job: &JobHandle, plan: &QuantizePlan) -> Result<QuantizeOutcome> {
    let output = Path::new(&plan.output_path);
    if output.exists() {
        std::fs::remove_file(output)?;
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = tokio::process::Command::new(&plan.tool_path);
    cmd.args(&plan.argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = std::time::Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(AtomError::Backend(format!(
                "could not start llama-quantize at {}: {e}",
                plan.tool_path
            )))
        }
    };

    job.log("info", format!("llama-quantize {}", plan.argv.join(" ")));
    job.progress("starting llama-quantize", 0.02);

    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();

    if let Some(stdout) = child.stdout.take() {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(("stdout".to_string(), line));
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(("stderr".to_string(), line));
            }
        });
    }
    drop(line_tx);

    let mut last_lines: Vec<String> = Vec::new();
    let exit_status;

    loop {
        tokio::select! {
            maybe = line_rx.recv() => {
                match maybe {
                    Some((stream, line)) => {
                        if let Some((done, total)) = parse_progress(&line) {
                            let fraction = 0.05 + 0.90 * (done as f64 / total as f64);
                            job.progress(&format!("quantizing tensor {done}/{total}"), fraction);
                            job.log(&stream, line.trim().to_string());
                        } else {
                            job.log(&stream, line.trim().to_string());
                        }
                        last_lines.push(line);
                        if last_lines.len() > 200 {
                            last_lines.remove(0);
                        }
                    }
                    None => {
                        // All output consumed; wait for the process itself.
                        exit_status = Some(child.wait().await);
                        break;
                    }
                }
                if job.is_cancelled() {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    cleanup_partial(output);
                    job.log("error", "cancelled by user; partial output removed");
                    return Err(job.cancelled_error());
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(400)) => {
                if job.is_cancelled() {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    cleanup_partial(output);
                    job.log("error", "cancelled by user; partial output removed");
                    return Err(job.cancelled_error());
                }
            }
        }
    }

    let status = match exit_status {
        Some(s) => s?,
        None => child.wait().await?,
    };
    let duration_ms = started.elapsed().as_millis() as u64;

    if !status.success() {
        cleanup_partial(output);
        let tail = last_lines
            .iter()
            .rev()
            .take(8)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(AtomError::Backend(format!(
            "llama-quantize exited with {} after {} ms; partial output removed.\n{}",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "a signal".into()),
            duration_ms,
            tail
        )));
    }

    if !output.is_file() {
        cleanup_partial(output);
        return Err(AtomError::Backend(
            "llama-quantize reported success but produced no output file".into(),
        ));
    }

    let produced_bytes = std::fs::metadata(output)?.len();
    if produced_bytes == 0 {
        cleanup_partial(output);
        return Err(AtomError::Backend("llama-quantize produced an empty file".into()));
    }

    job.progress("quantization finished, validating output", 0.97);
    Ok(QuantizeOutcome {
        output_path: plan.output_path.clone(),
        format: plan.format.clone(),
        exit_code: status.code().unwrap_or(-1),
        duration_ms,
        produced_bytes,
    })
}

fn cleanup_partial(output: &Path) {
    if output.exists() {
        let _ = std::fs::remove_file(output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tensor_progress_lines() {
        let line = "[  12/291]                  blk.0.attn_q.weight - [ 4096, 4096,     1,     1], type =    f16, converting to q4_K .. size =   128.00 MiB ->    40.50 MiB";
        assert_eq!(parse_progress(line), Some((12, 291)));
    }

    #[test]
    fn ignores_non_progress_lines() {
        assert_eq!(parse_progress("llama_model_quantize: loading model"), None);
        assert_eq!(parse_progress("[ 0/0] nothing"), None);
        assert_eq!(parse_progress("[ 5/2] backwards"), None);
    }
}
