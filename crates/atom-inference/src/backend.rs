use atom_security::find_executable;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub path: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackendStatus {
    pub server: Option<ToolInfo>,
    pub quantize: Option<ToolInfo>,
    pub cli: Option<ToolInfo>,
    /// ATOM needs llama-server for inference and llama-quantize for compression.
    pub inference_ready: bool,
    pub compression_ready: bool,
    pub checked_at: String,
    pub message: String,
}

fn version_of(bin: &PathBuf) -> Option<String> {
    let output = std::process::Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty() && (l.to_lowercase().contains("version") || l.contains(':')))
        .or_else(|| text.lines().find(|l| !l.trim().is_empty()))?;
    Some(line.trim().chars().take(160).collect())
}

fn tool(name: &'static str, explicit: Option<&str>) -> Option<ToolInfo> {
    let path = find_executable(name, explicit)?;
    Some(ToolInfo {
        name: name.to_string(),
        path: path.display().to_string(),
        version: version_of(&path),
    })
}

/// Detect the installed llama.cpp toolchain. Explicit config paths win; PATH is
/// the fallback. Nothing is downloaded or executed beyond `--version`.
pub fn probe(
    server_path: Option<&str>,
    quantize_path: Option<&str>,
    cli_path: Option<&str>,
) -> BackendStatus {
    let server = tool("llama-server", server_path);
    let quantize = tool("llama-quantize", quantize_path);
    let cli = tool("llama-cli", cli_path);

    let inference_ready = server.is_some();
    let compression_ready = quantize.is_some();
    let message = match (inference_ready, compression_ready) {
        (true, true) => "llama.cpp toolchain detected: inference and compression are available.".to_string(),
        (true, false) => "llama-server found; llama-quantize is missing, so permanent compression is unavailable."
            .to_string(),
        (false, true) => "llama-quantize found; llama-server is missing, so inference and benchmarking are unavailable."
            .to_string(),
        (false, false) => {
            "llama.cpp was not found. Install a llama.cpp build (llama-server, llama-quantize) or set \
             explicit binary paths in Settings. ATOM will not fake inference or compression without it."
                .to_string()
        }
    };

    BackendStatus {
        server,
        quantize,
        cli,
        inference_ready,
        compression_ready,
        checked_at: chrono::Local::now().to_rfc3339(),
        message,
    }
}

/// Extract quantize type names from `llama-quantize --help` output.
/// llama.cpp prints a table of `type  N` / `N or Q6_K` rows.
pub fn parse_quantize_formats(help: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let known = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_1", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_1", "Q4_0",
        "Q3_K_M", "Q3_K_S", "Q3_K_L", "Q2_K", "Q2_K_S", "IQ4_NL", "IQ4_XS", "IQ3_M", "IQ3_S",
        "IQ3_XS", "IQ2_S", "IQ2_XS", "IQ2_XXS", "IQ1_S", "IQ1_M", "BF16", "F16", "F32",
    ];
    for line in help.lines() {
        for name in known {
            // Only accept a token that is a standalone type name on the line.
            if line.split_whitespace().any(|tok| tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '_') == name)
                && !found.iter().any(|f| f == name)
            {
                found.push(name.to_string());
            }
        }
    }
    found
}
