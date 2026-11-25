//! ATOM desktop shell.
//!
//! The whole product runs in this process: the Rust core, the HTTP API and the
//! compiled React UI (embedded in the binary). The API binds an ephemeral
//! loopback port, the window loads that origin, and closing the window exits the
//! app, so nothing is left listening and no browser is involved.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atom_core::Paths;
use atom_engine::AppState;
use tauri::{WebviewUrl, WebviewWindowBuilder};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    let paths = Paths::resolve();
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
            let url: url::Url = origin.parse()?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("ATOM")
                .inner_size(1440.0, 940.0)
                .min_inner_size(1040.0, 680.0)
                .theme(Some(tauri::Theme::Dark))
                .build()?;
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

/// Report a fatal startup error. On Windows a console does not exist in a
/// release build, so the message is also shown in a native dialog.
fn fail(message: &str) {
    tracing::error!("{message}");
    eprintln!("{message}");
    rfd_error(message);
}

#[cfg(target_os = "linux")]
fn rfd_error(message: &str) {
    let _ = std::process::Command::new("zenity")
        .args(["--error", "--title", "ATOM", "--text", message])
        .status();
}

#[cfg(not(target_os = "linux"))]
fn rfd_error(message: &str) {
    let _ = message;
}
