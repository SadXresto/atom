//! ATOM desktop shell.
//!
//! The Rust core exposes the same HTTP API the browser build uses, bound to an
//! ephemeral loopback port. The window loads that origin, so the desktop app and
//! the web UI run identical code with no second implementation to drift.

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
            eprintln!("ATOM could not initialise its workspace: {err}");
            std::process::exit(1);
        }
    };
    state.spawn_sampler();

    // Bind before the window opens so the origin is known and ready.
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
    let port = std_listener.local_addr().expect("local addr").port();
    std_listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let router = atom_api::router(state);
    tauri::async_runtime::spawn(async move {
        let listener = tokio::net::TcpListener::from_std(std_listener).expect("adopt listener");
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!("local API stopped: {err}");
        }
    });

    let origin = format!("http://127.0.0.1:{port}");
    tauri::Builder::default()
        .setup(move |app| {
            let url: url::Url = origin.parse()?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("ATOM")
                .inner_size(1440.0, 940.0)
                .min_inner_size(1040.0, 680.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ATOM");
}
