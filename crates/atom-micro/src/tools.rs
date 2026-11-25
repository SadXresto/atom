//! External tool discovery for the Micro pipeline.
//!
//! ATOM never shells out through a shell: every tool is resolved to an absolute
//! path here and spawned with an explicit argv.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn find_program(name: &str) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let path = PathBuf::from(name);
        return if path.is_file() { Some(path) } else { None };
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if cfg!(windows) {
            for ext in ["exe", "cmd", "bat"] {
                let candidate = dir.join(format!("{name}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn version_output(program: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    let line = text.lines().next().unwrap_or("").trim().to_string();
    if line.is_empty() {
        None
    } else {
        Some(line.chars().take(120).collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolProbe {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub required_for: String,
    pub install_hint: String,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub platformio: ToolProbe,
    pub python: ToolProbe,
    pub pytorch: ToolProbe,
    pub esptool: ToolProbe,
    pub cc: ToolProbe,
    pub checked_at: String,
    pub note: String,
}

fn probe_python_torch() -> (ToolProbe, ToolProbe) {
    let python = find_program("python3").or_else(|| find_program("python"));
    let python_probe = ToolProbe {
        name: "python".into(),
        path: python.as_ref().map(|p| p.display().to_string()),
        version: python.as_ref().and_then(|p| version_output(p, &["--version"])),
        required_for: "Transformer and external trainer adapter".into(),
        install_hint: "Install Python 3.10+ and put `python3` on PATH.".into(),
        available: python.is_some(),
    };
    let torch_version = python.as_ref().and_then(|p| {
        let output = Command::new(p)
            .args(["-c", "import torch;print(torch.__version__)"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    });
    let torch = ToolProbe {
        name: "pytorch".into(),
        path: python.as_ref().map(|p| p.display().to_string()),
        version: torch_version.clone(),
        required_for: "Training transformer architectures".into(),
        install_hint: "pip install torch  (GPU builds follow the PyTorch install selector)".into(),
        available: torch_version.is_some(),
    };
    (python_probe, torch)
}

pub fn capabilities() -> Capabilities {
    let pio = find_program("pio");
    let esptool = find_program("esptool.py").or_else(|| find_program("esptool"));
    let cc = find_program("cc").or_else(|| find_program("gcc")).or_else(|| find_program("clang"));
    let (python, torch) = probe_python_torch();
    Capabilities {
        platformio: ToolProbe {
            name: "platformio".into(),
            path: pio.as_ref().map(|p| p.display().to_string()),
            version: pio.as_ref().and_then(|p| version_output(p, &["--version"])),
            required_for: "Compiling and flashing firmware".into(),
            install_hint: "pip install platformio  →  puts `pio` on PATH".into(),
            available: pio.is_some(),
        },
        python,
        pytorch: torch,
        esptool: ToolProbe {
            name: "esptool".into(),
            path: esptool.as_ref().map(|p| p.display().to_string()),
            version: esptool.as_ref().and_then(|p| version_output(p, &["version"])),
            required_for: "Direct ESP32 serial flashing without PlatformIO".into(),
            install_hint: "pip install esptool".into(),
            available: esptool.is_some(),
        },
        cc: ToolProbe {
            name: "cc".into(),
            path: cc.as_ref().map(|p| p.display().to_string()),
            version: cc.as_ref().and_then(|p| version_output(p, &["--version"])),
            required_for: "Compiling exported reference C code on the host".into(),
            install_hint: "Install a C compiler (build-essential / Xcode command line tools).".into(),
            available: cc.is_some(),
        },
        checked_at: chrono::Local::now().to_rfc3339(),
        note: "Probed from PATH at request time. Missing tools disable exactly one step and say what is required.".into(),
    }
}

/// Run an external tool, capturing combined output with a hard timeout enforced
/// by killing the child. Returns (status_success, lines).
pub fn run_tool(
    program: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    mut on_line: impl FnMut(&str),
    cancel: impl Fn() -> bool,
    timeout: std::time::Duration,
) -> std::io::Result<bool> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(stream) = stdout {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(std::result::Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        }));
    }
    if let Some(stream) = stderr {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(std::result::Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        }));
    }
    drop(tx);
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        while let Ok(line) = rx.try_recv() {
            on_line(&line);
        }
        if cancel() {
            cancelled = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        if started.elapsed() > timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    };
    while let Ok(line) = rx.try_recv() {
        on_line(&line);
    }
    for handle in readers {
        let _ = handle.join();
    }
    if cancelled {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
    }
    if timed_out {
        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "tool timed out"));
    }
    Ok(status.map(|s| s.success()).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_is_none() {
        assert!(find_program("atom-definitely-not-a-real-binary").is_none());
    }

    #[test]
    fn capabilities_serialize() {
        let caps = capabilities();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("platformio"));
    }
}
