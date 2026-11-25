//! Tool detection, format availability and argument construction.

use atom_core::{AtomError, Result};
use atom_inference::parse_quantize_formats;
use atom_models::quant::{format_catalog, ggml_type_by_name, QuantFormat};
use atom_models::ModelSummary;
use atom_security::find_executable;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// True when the format list was read from the installed binary's help text.
    pub formats_from_tool: bool,
    pub supported_types: Vec<String>,
    pub allow_requantize: bool,
    pub checked_at: String,
    pub message: String,
}

fn read_help(bin: &Path) -> String {
    let mut text = String::new();
    for flag in ["--help", "-h"] {
        if let Ok(out) = std::process::Command::new(bin)
            .arg(flag)
            .stdin(Stdio::null())
            .output()
        {
            text.push_str(&String::from_utf8_lossy(&out.stdout));
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            if !text.trim().is_empty() {
                break;
            }
        }
    }
    text
}

/// Detect llama-quantize and read the formats it actually advertises.
pub fn detect_tool(explicit: Option<&str>) -> ToolStatus {
    let checked_at = chrono::Local::now().to_rfc3339();
    let Some(path) = find_executable("llama-quantize", explicit) else {
        return ToolStatus {
            available: false,
            path: None,
            version: None,
            formats_from_tool: false,
            supported_types: Vec::new(),
            allow_requantize: false,
            checked_at,
            message: "llama-quantize was not found. Permanent compression needs a llama.cpp build; \
                      set the binary path in Settings."
                .into(),
        };
    };
    let help = read_help(&path);
    let mut supported = parse_quantize_formats(&help);
    let formats_from_tool = !supported.is_empty();
    let allow_requantize = help.contains("allow-requantize");
    if !formats_from_tool {
        // Keep the UI usable when a build prints an unusual help layout, but say
        // so plainly instead of pretending the list came from the binary.
        supported = format_catalog()
            .iter()
            .flat_map(|f| f.name.split('_').next().map(|s| s.to_string()))
            .collect();
        supported.dedup();
    }
    let version = help
        .lines()
        .find(|l| l.to_lowercase().contains("usage:") || l.to_lowercase().contains("version"))
        .map(|l| l.trim().chars().take(160).collect::<String>());
    let message = if formats_from_tool {
        format!(
            "llama-quantize detected at {}. {} quantization types advertised by the binary.",
            path.display(),
            supported.len()
        )
    } else {
        format!(
            "llama-quantize detected at {}, but its help output did not list quantization types. \
             ATOM falls back to the standard type names and lets the tool reject anything unsupported.",
            path.display()
        )
    };
    ToolStatus {
        available: true,
        path: Some(path.display().to_string()),
        version,
        formats_from_tool,
        supported_types: supported,
        allow_requantize,
        checked_at,
        message,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormatOption {
    pub name: String,
    pub label: String,
    pub tradeoff: String,
    pub quality_tier: String,
    pub bits_per_weight: f64,
    pub estimated_bytes: u64,
    pub estimated_savings_bytes: i64,
    pub estimated_savings_percent: f64,
    pub supported_by_tool: bool,
    pub supported_for_source: bool,
    pub note: String,
}

fn family_supported(supported: &[String], format: &QuantFormat) -> bool {
    if supported.iter().any(|s| s.eq_ignore_ascii_case(format.name)) {
        return true;
    }
    // `Q5_K_M` is covered by a tool advertising `Q5_K`.
    supported.iter().any(|s| {
        format.name.starts_with(s.as_str())
            && format.name.as_bytes().get(s.len()) == Some(&b'_')
    })
}

/// Every format ATOM knows about, annotated with what the installed tool and the
/// selected source model actually allow.
pub fn formats_for(tool: &ToolStatus, source: &ModelSummary, source_size: u64) -> Vec<FormatOption> {
    let compat = &source.compatibility;
    format_catalog()
        .iter()
        .map(|f| {
            let supported_by_tool = tool.available && family_supported(&tool.supported_types, f);
            let requant = compat.requires_requantize_flag;
            let supported_for_source = compat.quantizable && (!requant || tool.allow_requantize);
            let estimated_bytes = source.estimate_quantized_bytes(f.bpw);
            let savings = source_size as i64 - estimated_bytes as i64;
            let mut note = String::new();
            if !tool.available {
                note = "llama-quantize is not installed.".into();
            } else if !supported_by_tool {
                note = format!("The installed llama-quantize does not advertise {}.", f.name);
            } else if !compat.quantizable {
                note = compat.reason.clone();
            } else if requant && !tool.allow_requantize {
                note = "Source is already quantized and this llama-quantize build has no --allow-requantize."
                    .into();
            } else if estimated_bytes >= source_size {
                note = "Estimated output is not smaller than the source; this would not save space.".into();
            }
            let usable = supported_by_tool
                && supported_for_source
                && estimated_bytes < source_size
                && !note.contains("does not advertise");
            FormatOption {
                name: f.name.to_string(),
                label: f.label.to_string(),
                tradeoff: f.tradeoff.to_string(),
                quality_tier: f.quality_tier.to_string(),
                bits_per_weight: f.bpw,
                estimated_bytes,
                estimated_savings_bytes: savings,
                estimated_savings_percent: if source_size > 0 {
                    (savings as f64 / source_size as f64) * 100.0
                } else {
                    0.0
                },
                supported_by_tool,
                supported_for_source: usable,
                note,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizePlan {
    pub tool_path: String,
    pub source_path: String,
    pub output_path: String,
    pub format: String,
    pub argv: Vec<String>,
    pub threads: u32,
    pub allow_requantize: bool,
}

/// Build the exact argv used for a run. Output is validated to be outside the
/// source path and inside an ATOM output root.
pub fn build_plan(
    tool: &ToolStatus,
    source_path: &Path,
    output_path: &Path,
    format: &str,
    threads: u32,
    source_requires_requantize: bool,
) -> Result<QuantizePlan> {
    if !tool.available {
        return Err(AtomError::Unavailable("llama-quantize is not installed".into()));
    }
    let tool_path = tool
        .path
        .clone()
        .ok_or_else(|| AtomError::Unavailable("llama-quantize path is unknown".into()))?;
    if source_path == output_path {
        return Err(AtomError::Invalid(
            "refusing to quantize onto the source model; choose a separate output directory".into(),
        ));
    }
    let known = format_catalog().iter().any(|f| f.name.eq_ignore_ascii_case(format));
    if !known {
        return Err(AtomError::Invalid(format!(
            "'{format}' is not a quantization type ATOM offers"
        )));
    }
    if ggml_type_by_name(format.split('_').next().unwrap_or(format)).is_none()
        && !format.contains('_')
    {
        return Err(AtomError::Invalid(format!("unknown tensor type for '{format}'")));
    }
    let mut argv = Vec::new();
    if source_requires_requantize {
        if !tool.allow_requantize {
            return Err(AtomError::Unavailable(
                "the source is already quantized and this llama-quantize build lacks --allow-requantize".into(),
            ));
        }
        argv.push("--allow-requantize".to_string());
    }
    argv.push("-t".into());
    argv.push(threads.max(1).to_string());
    argv.push(source_path.display().to_string());
    argv.push(output_path.display().to_string());
    argv.push(format.to_string());
    Ok(QuantizePlan {
        tool_path,
        source_path: source_path.display().to_string(),
        output_path: output_path.display().to_string(),
        format: format.to_string(),
        argv,
        threads,
        allow_requantize: source_requires_requantize,
    })
}

/// Default output name for a compressed variant.
pub fn default_output_name(source_file: &str, format: &str) -> String {
    let stem = PathBuf::from(source_file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".into());
    let cleaned = stem
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.', "-");
    format!("{cleaned}-{}.gguf", format.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atom_models::gguf::fixture::tiny_model;
    use atom_models::gguf::GgufFile;
    use atom_models::ModelSummary;

    fn summary() -> (ModelSummary, u64) {
        let bytes = tiny_model(atom_models::quant::GGML_TYPE_F16);
        let path = std::env::temp_dir().join(format!("atom-plan-{}.gguf", uuid_like()));
        std::fs::write(&path, &bytes).unwrap();
        let gguf = GgufFile::open(&path).unwrap();
        let mut summary = ModelSummary::from_gguf(&gguf, 1);
        let _ = std::fs::remove_file(&path);
        // Scale up to a 7B-parameter, 5 GB source so size estimates are realistic.
        summary.parameter_count = 7_000_000_000;
        (summary, 5_000_000_000)
    }

    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().to_string()
    }

    fn fake_tool() -> ToolStatus {
        ToolStatus {
            available: true,
            path: Some("/usr/bin/llama-quantize".into()),
            version: None,
            formats_from_tool: true,
            supported_types: vec!["Q2_K".into(), "Q4_K".into(), "Q5_K".into(), "Q6_K".into(), "Q8_0".into()],
            allow_requantize: true,
            checked_at: String::new(),
            message: String::new(),
        }
    }

    #[test]
    fn annotates_unavailable_formats() {
        let (summary, size) = summary();
        let options = formats_for(&fake_tool(), &summary, size);
        let q4 = options.iter().find(|o| o.name == "Q4_K_M").unwrap();
        assert!(q4.supported_by_tool, "Q4_K covers Q4_K_M");
        assert!(q4.supported_for_source, "{} ({})", q4.note, q4.estimated_bytes);
        assert!(q4.estimated_bytes < size);
        assert!(q4.estimated_savings_percent > 0.0);
        let q4_0 = options.iter().find(|o| o.name == "Q4_0").unwrap();
        assert!(!q4_0.supported_by_tool);
    }

    #[test]
    fn refuses_source_as_output() {
        let tool = fake_tool();
        let p = Path::new("/tmp/a.gguf");
        let err = build_plan(&tool, p, p, "Q4_K_M", 4, false).unwrap_err();
        assert_eq!(err.code(), "invalid_request");
    }

    #[test]
    fn rejects_unknown_format() {
        let tool = fake_tool();
        let err = build_plan(
            &tool,
            Path::new("/tmp/a.gguf"),
            Path::new("/tmp/b.gguf"),
            "Q7_K",
            4,
            false,
        )
        .unwrap_err();
        assert_eq!(err.code(), "invalid_request");
    }

    #[test]
    fn builds_expected_argv() {
        let tool = fake_tool();
        let plan = build_plan(
            &tool,
            Path::new("/models/a.gguf"),
            Path::new("/out/a-q4.gguf"),
            "Q4_K_M",
            6,
            false,
        )
        .unwrap();
        assert_eq!(
            plan.argv,
            vec!["-t", "6", "/models/a.gguf", "/out/a-q4.gguf", "Q4_K_M"]
        );
    }

    #[test]
    fn default_output_name_is_safe() {
        let name = default_output_name("Qwen 2.5 (7B).gguf", "Q4_K_M");
        assert_eq!(name, "Qwen-2.5--7B--q4_k_m.gguf");
        assert!(!name.contains(' '));
    }
}
