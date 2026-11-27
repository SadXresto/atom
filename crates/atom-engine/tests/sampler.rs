//! The desktop shell starts the memory sampler from `main`, before Tauri has
//! created a Tokio runtime. A bare `tokio::spawn` there panicked with "there is
//! no reactor running" and killed the app at startup (0.2.0 on Windows), so the
//! sampler must cope with a caller that has no runtime.

use atom_core::Paths;
use atom_engine::AppState;

#[test]
fn sampler_starts_without_a_runtime() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = AppState::new(Paths::from_root(dir.path())).expect("engine");
    // No #[tokio::test] here on purpose: this thread has no runtime.
    state.spawn_sampler();
    std::thread::sleep(std::time::Duration::from_millis(200));
    // Reaching this point means the sampler did not panic the process.
    assert_eq!(state.paths().root, dir.path());
}
