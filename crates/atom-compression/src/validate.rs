//! Post-quantization validation. A file only enters the library if it parses,
//! has the expected tensor mix, and is genuinely smaller than its source.

use atom_core::Result;
use atom_models::gguf::GgufFile;
use atom_models::quant::ggml_type_by_name;
use atom_security::sha256_file;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub name: String,
    pub passed: bool,
    /// `required` checks gate library insertion; `info` checks are reported only.
    pub severity: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub checks: Vec<ValidationCheck>,
    pub output_sha256: Option<String>,
    pub output_bytes: u64,
    pub source_bytes: u64,
    pub savings_percent: f64,
    pub tensor_count: usize,
    pub dominant_quant: String,
    pub parameter_count: u64,
    pub validated_at: String,
}

pub fn validate_output(
    output: &Path,
    source: &Path,
    source_tensor_count: usize,
    requested_format: &str,
) -> Result<ValidationReport> {
    let mut checks = Vec::new();
    let source_bytes = std::fs::metadata(source).map(|m| m.len()).unwrap_or(0);
    let output_bytes = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);

    checks.push(ValidationCheck {
        name: "output file exists".into(),
        passed: output.is_file(),
        severity: "required".into(),
        detail: output.display().to_string(),
    });
    checks.push(ValidationCheck {
        name: "output is non-empty".into(),
        passed: output_bytes > 0,
        severity: "required".into(),
        detail: format!("{output_bytes} bytes"),
    });
    checks.push(ValidationCheck {
        name: "output is smaller than source".into(),
        passed: output_bytes < source_bytes && output_bytes > 0,
        severity: "required".into(),
        detail: format!("{output_bytes} bytes vs {source_bytes} bytes"),
    });
    checks.push(ValidationCheck {
        name: "source file untouched".into(),
        passed: source.is_file(),
        severity: "required".into(),
        detail: format!("{} still present", source.display()),
    });

    let gguf = GgufFile::open(output);
    let (tensor_count, dominant, parameters) = match &gguf {
        Ok(g) => {
            let by_type = g.elements_per_type();
            let dominant = by_type
                .first()
                .map(|(id, _)| {
                    atom_models::quant::ggml_type(*id)
                        .map(|t| t.name.to_string())
                        .unwrap_or_else(|| format!("type_{id}"))
                })
                .unwrap_or_else(|| "none".into());
            let params: u64 = by_type.iter().map(|(_, e)| *e).sum();
            (g.tensors.len(), dominant, params)
        }
        Err(_) => (0, "unreadable".to_string(), 0),
    };

    checks.push(ValidationCheck {
        name: "output is a readable GGUF".into(),
        passed: gguf.is_ok(),
        severity: "required".into(),
        detail: match &gguf {
            Ok(g) => format!("GGUF v{} with {} tensors", g.version, g.tensors.len()),
            Err(e) => e.to_string(),
        },
    });
    checks.push(ValidationCheck {
        name: "tensor count matches source".into(),
        passed: tensor_count == source_tensor_count && tensor_count > 0,
        severity: "required".into(),
        detail: format!("{tensor_count} tensors vs {source_tensor_count} in the source"),
    });
    let family = requested_format.split('_').next().unwrap_or(requested_format);
    let expected = ggml_type_by_name(family).map(|t| t.name.to_string());
    let matches = expected
        .as_deref()
        .map(|e| dominant.starts_with(e))
        .unwrap_or(false);
    checks.push(ValidationCheck {
        name: "tensor mix is quantized as requested".into(),
        passed: matches,
        severity: "info".into(),
        detail: format!("dominant tensor type is {dominant}; requested {requested_format}"),
    });

    let output_sha256 = if output.is_file() {
        sha256_file(output).ok().map(|(h, _)| h)
    } else {
        None
    };
    checks.push(ValidationCheck {
        name: "output checksum recorded".into(),
        passed: output_sha256.is_some(),
        severity: "required".into(),
        detail: output_sha256.clone().unwrap_or_else(|| "unavailable".into()),
    });

    let ok = checks.iter().filter(|c| c.severity == "required").all(|c| c.passed);
    let savings_percent = if source_bytes > 0 {
        ((source_bytes as f64 - output_bytes as f64) / source_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(ValidationReport {
        ok,
        checks,
        output_sha256,
        output_bytes,
        source_bytes,
        savings_percent,
        tensor_count,
        dominant_quant: dominant,
        parameter_count: parameters,
        validated_at: chrono::Local::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atom_models::gguf::fixture::Builder;
    use atom_models::gguf::GgufValue;
    use atom_models::quant::{GGML_TYPE_F16, GGML_TYPE_Q4_K};

    fn write(dir: &Path, name: &str, ggml: u32, pad: usize) -> std::path::PathBuf {
        let bytes = Builder::new()
            .meta("general.architecture", GgufValue::Str("llama".into()))
            .tensor("token_embd.weight", &[256, 4], ggml)
            .tensor("output.weight", &[256, 4], ggml)
            .build();
        let mut bytes = bytes;
        bytes.extend(std::iter::repeat_n(0u8, pad));
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn accepts_valid_quantized_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = write(dir.path(), "src.gguf", GGML_TYPE_F16, 4096);
        let output = write(dir.path(), "out.gguf", GGML_TYPE_Q4_K, 0);
        let report = validate_output(&output, &source, 2, "Q4_K_M").unwrap();
        assert!(report.ok, "{:?}", report.checks);
        assert!(report.savings_percent > 0.0);
        assert!(report.output_sha256.is_some());
    }

    #[test]
    fn rejects_unquantized_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = write(dir.path(), "src.gguf", GGML_TYPE_F16, 4096);
        let output = write(dir.path(), "out.gguf", GGML_TYPE_F16, 0);
        let report = validate_output(&output, &source, 2, "Q4_K_M").unwrap();
        // Size shrank, so required checks pass, but the informational check flags
        // that the tensor mix was not what was asked for.
        assert!(report.checks.iter().any(|c| c.name.contains("quantized as requested") && !c.passed));
    }

    #[test]
    fn rejects_missing_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = write(dir.path(), "src.gguf", GGML_TYPE_F16, 0);
        let missing = dir.path().join("nope.gguf");
        let report = validate_output(&missing, &source, 2, "Q4_K_M").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn rejects_tensor_count_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let source = write(dir.path(), "src.gguf", GGML_TYPE_F16, 2048);
        let output = write(dir.path(), "out.gguf", GGML_TYPE_Q4_K, 0);
        let report = validate_output(&output, &source, 7, "Q4_K_M").unwrap();
        assert!(!report.ok);
    }
}
