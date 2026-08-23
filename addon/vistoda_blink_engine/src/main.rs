use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use vistoda_blink_engine::{AppConfig, Cli, EngineState, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let cli = Cli::parse();
    let config = AppConfig::load(cli.config).await?;
    let listener = TcpListener::bind(cli.listen).await?;
    let state = EngineState::new(config.token, config.credentials_path)?;
    match state.initialize().await {
        Ok(true) => tracing::info!("restored standalone Blink enrollment"),
        Ok(false) => tracing::info!("waiting for standalone Blink enrollment"),
        Err(error) => tracing::warn!(%error, "could not restore Blink enrollment"),
    }
    tracing::info!(listen = %cli.listen, version = env!("CARGO_PKG_VERSION"), "engine ready");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "could not install Ctrl-C handler");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "could not install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = ctrl_c => {}, () = terminate => {} }
}
