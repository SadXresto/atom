//! ATOM desktop shell.
//!
//! The whole product runs in this process: the Rust core, the HTTP API and the
//! compiled React UI (embedded in the binary). The API binds an ephemeral
//! loopback port, the window loads that origin, and closing the window exits the
//! app, so nothing is left listening and no browser is involved.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atom_core::Paths;
use atom_engine::AppState;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{WebviewUrl, WebviewWindowBuilder};

fn main() {
    std::panic::set_hook(Box::new(|info| {
        fail(&format!(
            "ATOM stopped unexpectedly.\n\n{info}\n\nDetails were written to:\n{}",
            log_path().display()
        ));
    }));

    let log = LogWriter::open(&log_path());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .with_writer(move || log.clone())
        .init();
    tracing::info!(
        "ATOM {} starting; log file: {}",
        env!("CARGO_PKG_VERSION"),
        log_path().display()
    );

    let paths = Paths::resolve();
    let webview_dir = paths.root.join("webview");
    let state = match AppState::new(paths) {
        Ok(state) => state,
        Err(err) => {
            fail(&format!("ATOM could not initialise its workspace:\n\n{err}"));
            return;
        }
    };
    state.spawn_sampler();

    // Bind before the window opens so the origin is known, listening and ready
    // for the very first request the webview makes.
    let std_listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(err) => {
            fail(&format!("ATOM could not start its local service:\n\n{err}"));
            return;
        }
    };
    let port = match std_listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(err) => {
            fail(&format!("ATOM could not read its local address:\n\n{err}"));
            return;
        }
    };
    if let Err(err) = std_listener.set_nonblocking(true) {
        fail(&format!("ATOM could not configure its local service:\n\n{err}"));
        return;
    }

    let router = atom_api::router(state);
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::from_std(std_listener) {
            Ok(listener) => {
                if let Err(err) = axum::serve(listener, router).await {
                    tracing::error!("local API stopped: {err}");
                }
            }
            Err(err) => tracing::error!("could not adopt the local listener: {err}"),
        }
    });

    let origin = format!("http://127.0.0.1:{port}");
    tracing::info!("ATOM desktop UI served from {origin}");

    let app = tauri::Builder::default()
        .setup(move |app| {
            // Tauri's own docs: creating a webview on the main thread inside a
            // synchronous handler deadlocks on Windows (wry#583). Build it on a
            // worker thread there so the event loop starts and the window can
            // appear. GTK requires the main thread, so Linux keeps the
            // synchronous path.
            #[cfg(target_os = "windows")]
            {
                let handle = app.handle().clone();
                let origin = origin.clone();
                let webview_dir = webview_dir.clone();
                std::thread::spawn(move || {
                    if let Err(err) = open_window(&handle, &origin, &webview_dir) {
                        fail(&format!(
                            "ATOM could not open its window.\n\n{err}\n\nDetails were written to:\n{}{}",
                            log_path().display(),
                            webview_hint()
                        ));
                        std::process::exit(1);
                    }
                });
            }
            #[cfg(not(target_os = "windows"))]
            if let Err(err) = open_window(app.handle(), &origin, &webview_dir) {
                fail(&format!("ATOM could not open its window:\n\n{err}"));
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    match app {
        Ok(app) => {
            app.run(|_handle, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event {
                    tracing::info!("ATOM shutting down");
                }
            });
        }
        Err(err) => fail(&format!("ATOM could not open its window:\n\n{err}")),
    }
}

fn open_window(
    app: &tauri::AppHandle,
    origin: &str,
    data_dir: &Path,
) -> std::result::Result<(), String> {
    let url: url::Url = origin.parse().map_err(|err| format!("{err}"))?;
    // WebView2 needs a writable profile directory; creating it up front avoids
    // the "window opens and closes instantly" failure when it cannot be made.
    let _ = std::fs::create_dir_all(data_dir);
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .title("ATOM")
        .inner_size(1440.0, 940.0)
        .min_inner_size(1040.0, 680.0)
        .theme(Some(tauri::Theme::Dark));
    #[cfg(target_os = "windows")]
    let builder = builder.data_directory(data_dir.to_path_buf());
    builder
        .build()
        .map(|_| ())
        .map_err(|err| format!("{err}"))
}

/// Report a fatal startup error. On Windows a console does not exist in a
/// release build, so the message is also shown in a native dialog and appended
/// to `%USERPROFILE%\.atom\logs\desktop.log`.
fn fail(message: &str) {
    log_line(message);
    show_error(message);
}

fn log_path() -> PathBuf {
    Paths::resolve().log_dir().join("desktop.log")
}

fn log_line(message: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{stamp}] {message}");
    }
}

/// A missing WebView2 runtime is the most common reason a window never appears
/// on Windows, so add an actionable line to the error. This only augments the
/// error; it never blocks a machine that has the runtime in a non-standard place.
fn webview_hint() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        if !webview_runtime_present() {
            return "\n\nATOM needs the Microsoft Edge WebView2 Runtime, which does not appear \
                    to be installed. Install the Evergreen Runtime from \
                    https://developer.microsoft.com/microsoft-edge/webview2/ and start ATOM again.";
        }
    }
    ""
}

#[cfg(target_os = "windows")]
fn webview_runtime_present() -> bool {
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| {
            PathBuf::from(root)
                .join("Microsoft")
                .join("EdgeWebView")
                .join("Application")
        })
        .any(|dir| dir.is_dir())
}

#[cfg(target_os = "windows")]
fn show_error(message: &str) {
    use std::os::windows::ffi::OsStrExt;

    let text: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let title: Vec<u16> = "ATOM".encode_utf16().chain(std::iter::once(0)).collect();

    // MB_OK | MB_ICONERROR | MB_SETFOREGROUND
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x0001_0010);
    }
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        flags: u32,
    ) -> i32;
}

#[cfg(target_os = "linux")]
fn show_error(message: &str) {
    let _ = std::process::Command::new("zenity")
        .args(["--error", "--title", "ATOM", "--text", message])
        .status();
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn show_error(_message: &str) {}

/// Writes tracing output to the desktop log. A failed write is ignored: release
/// Windows builds have no console and logging must never stop the app.
#[derive(Clone, Default)]
struct LogWriter(Option<Arc<Mutex<std::fs::File>>>);

impl LogWriter {
    fn open(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok();
        LogWriter(file.map(|file| Arc::new(Mutex::new(file))))
    }
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(file) = &self.0 {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(buf);
            }
        }
        let _ = std::io::stderr().write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.0 {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
        Ok(())
    }
}
