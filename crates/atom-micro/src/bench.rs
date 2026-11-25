//! Import and validation of benchmark results measured on physical hardware.
//!
//! ATOM does not run on the microcontroller, so it cannot measure those numbers.
//! It accepts a JSON report produced by the exported firmware's benchmark mode,
//! validates every field, and stores exactly what was measured. No number is
//! ever synthesised when a device is absent.

use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::project::DeviceBenchmark;

#[derive(Deserialize)]
struct RawReport {
    device: Option<String>,
    firmware: Option<String>,
    #[serde(default)]
    runs: Vec<RawRun>,
}

#[derive(Deserialize)]
struct RawRun {
    label: Option<String>,
    tokens_per_second: Option<f64>,
    latency_ms: Option<f64>,
    ram_bytes: Option<u64>,
    flash_bytes: Option<u64>,
    notes: Option<String>,
    measured_at: Option<String>,
}

/// Expected shape, shown in the UI so a device report can be produced by hand.
pub fn schema_example() -> serde_json::Value {
    serde_json::json!({
        "device": "ESP32-S3 DevKitC-1",
        "firmware": "atom-micro 0.1.0",
        "runs": [{
            "label": "int8 · greedy",
            "tokens_per_second": 41.2,
            "latency_ms": 24.3,
            "ram_bytes": 118784,
            "flash_bytes": 2163456,
            "notes": "measured with the firmware benchmark command",
            "measured_at": "2026-11-24T18:12:00Z"
        }]
    })
}

pub fn parse_report(path: &Path, source: &Path) -> Result<(String, String, Vec<DeviceBenchmark>)> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > 4 * 1024 * 1024 {
        return Err(AtomError::Invalid("benchmark report is larger than 4 MiB".into()));
    }
    let raw = std::fs::read_to_string(path)?;
    let report: RawReport = serde_json::from_str(&raw)
        .map_err(|e| AtomError::Invalid(format!("benchmark report is not valid JSON: {e}")))?;
    let device = report.device.unwrap_or_else(|| "unknown device".into());
    let firmware = report.firmware.unwrap_or_else(|| "unknown firmware".into());
    if report.runs.is_empty() {
        return Err(AtomError::Invalid(
            "the report contains no runs; at least one measured run is required".into(),
        ));
    }
    let imported_at = chrono::Local::now().to_rfc3339();
    let mut out = Vec::new();
    for (index, run) in report.runs.into_iter().enumerate() {
        let tps = run.tokens_per_second.ok_or_else(|| {
            AtomError::Invalid(format!("run {} has no tokens_per_second", index + 1))
        })?;
        let latency = run.latency_ms.ok_or_else(|| {
            AtomError::Invalid(format!("run {} has no latency_ms", index + 1))
        })?;
        if !tps.is_finite() || tps <= 0.0 {
            return Err(AtomError::Invalid(format!(
                "run {} reports tokens_per_second={tps}; a measured positive rate is required",
                index + 1
            )));
        }
        if !latency.is_finite() || latency <= 0.0 {
            return Err(AtomError::Invalid(format!(
                "run {} reports latency_ms={latency}; a measured positive latency is required",
                index + 1
            )));
        }
        let ram = run.ram_bytes.unwrap_or(0);
        let flash = run.flash_bytes.unwrap_or(0);
        if ram == 0 && flash == 0 {
            return Err(AtomError::Invalid(format!(
                "run {} reports neither ram_bytes nor flash_bytes; device memory use must be measured",
                index + 1
            )));
        }
        out.push(DeviceBenchmark {
            id: uuid::Uuid::new_v4().to_string(),
            device: device.clone(),
            firmware: firmware.clone(),
            label: run.label.unwrap_or_else(|| format!("run {}", index + 1)),
            tokens_per_second: tps,
            latency_ms: latency,
            ram_bytes: ram,
            flash_bytes: flash,
            notes: run.notes.unwrap_or_default(),
            measured_at: run.measured_at,
            imported_at: imported_at.clone(),
            source: source.display().to_string(),
        });
    }
    Ok((device, firmware, out))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub device: String,
    pub firmware: String,
    pub best_tokens_per_second: f64,
    pub mean_tokens_per_second: f64,
    pub mean_latency_ms: f64,
    pub peak_ram_bytes: u64,
    pub runs: usize,
}

pub fn summarize(runs: &[DeviceBenchmark]) -> Option<BenchmarkSummary> {
    if runs.is_empty() {
        return None;
    }
    let best = runs.iter().map(|r| r.tokens_per_second).fold(0.0, f64::max);
    let mean = runs.iter().map(|r| r.tokens_per_second).sum::<f64>() / runs.len() as f64;
    let latency = runs.iter().map(|r| r.latency_ms).sum::<f64>() / runs.len() as f64;
    let peak = runs.iter().map(|r| r.ram_bytes).max().unwrap_or(0);
    Some(BenchmarkSummary {
        device: runs[0].device.clone(),
        firmware: runs[0].firmware.clone(),
        best_tokens_per_second: best,
        mean_tokens_per_second: mean,
        mean_latency_ms: latency,
        peak_ram_bytes: peak,
        runs: runs.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn valid_report_is_imported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bench.json");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(
            file,
            r#"{{"device":"ESP32-S3","firmware":"atom 0.1","runs":[{{"label":"q8","tokens_per_second":33.5,"latency_ms":29.8,"ram_bytes":90000,"flash_bytes":2100000}}]}}"#
        )
        .unwrap();
        let (device, firmware, runs) = parse_report(&path, &path).unwrap();
        assert_eq!(device, "ESP32-S3");
        assert_eq!(firmware, "atom 0.1");
        assert_eq!(runs.len(), 1);
        let summary = summarize(&runs).unwrap();
        assert!((summary.best_tokens_per_second - 33.5).abs() < 1e-6);
    }

    #[test]
    fn missing_measurements_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(
            &path,
            r#"{"device":"x","runs":[{"label":"a","tokens_per_second":0,"latency_ms":10,"ram_bytes":1}]}"#,
        )
        .unwrap();
        assert!(parse_report(&path, &path).is_err());
    }
}
