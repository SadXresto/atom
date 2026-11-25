use atom_core::Paths;
use atom_engine::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,atom_server=debug".into()),
        )
        .with_target(false)
        .init();

    let paths = Paths::resolve();
    let state = AppState::new(paths)?;
    state.spawn_sampler();
    let port = std::env::var("ATOM_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or_else(|| state.config().api_port);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let app = atom_api::router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("ATOM API listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
