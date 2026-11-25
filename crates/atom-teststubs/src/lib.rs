//! Test-only stub executables that behave like `llama-quantize` and
//! `llama-server` closely enough to exercise ATOM's process supervision,
//! progress parsing, cancellation cleanup, output validation and SSE streaming
//! without a real llama.cpp build or a real model.
//!
//! These binaries are compiled by `cargo test` and found next to the test
//! executable. They are never installed: the crate is `publish = false` and is
//! only a dev-dependency.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static BUILD: Once = Once::new();

/// Absolute path of a stub executable built in the current Cargo target dir.
///
/// Panics with an actionable message when the binary is missing, because a
/// silently skipped test would hide exactly the behaviour these stubs exist to
/// prove.
pub fn bin(name: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("test executable path is available");
    // The test binary can live in `<target>/<profile>/deps` or in a Cargo build
    // script directory, while the stub lands in `<target>/<profile>`. Walk up a
    // few directories and take the first match instead of assuming a layout.
    let mut dir = exe.parent().expect("test executable has a parent directory");
    for _ in 0..6 {
        let candidate = dir.join(exe_name(name));
        if candidate.is_file() {
            return candidate;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    // Cargo only builds a dependency's binaries when the whole workspace is
    // built, so a `cargo test -p <crate>` run can start without them. Build
    // them once, on demand, instead of failing the test run.
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let status = Command::new(cargo)
            .args(["build", "-p", "atom-teststubs", "--bins", "--quiet"])
            .current_dir(&root)
            .status();
        match status {
            Ok(status) if status.success() => {}
            other => panic!("could not build the stub binaries: {other:?}"),
        }
    });
    let mut dir = exe.parent().expect("test executable has a parent directory");
    for _ in 0..6 {
        let candidate = dir.join(exe_name(name));
        if candidate.is_file() {
            return candidate;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    panic!("stub binary '{name}' is still missing after building atom-teststubs")
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}
