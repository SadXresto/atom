//! Test-only stub executables that behave like `llama-quantize` and
//! `llama-server` closely enough to exercise ATOM's process supervision,
//! progress parsing, cancellation cleanup, output validation and SSE streaming
//! without a real llama.cpp build or a real model.
//!
//! These binaries are compiled by `cargo test` and found next to the test
//! executable. They are never installed: the crate is `publish = false` and is
//! only a dev-dependency.

use std::path::PathBuf;

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
    panic!(
        "stub binary '{name}' was not built. Run `cargo build -p atom-teststubs` first \
         (or `cargo test --workspace`, which builds it)."
    )
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}
