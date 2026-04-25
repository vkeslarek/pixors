use clap::Parser;
use pixors_engine::config::{load_from, CliConfig};
use pixors_engine::server::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_from(CliConfig::parse());

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from(&cfg.max_level))
        .init();

    let addr = format!("127.0.0.1:{}", cfg.engine.port);
    tracing::info!("Starting Pixors engine server on {}", addr);
    match start_server(&addr).await {
        Ok(()) => tracing::info!("Server exited normally"),
        Err(e) => {
            tracing::error!("Server failed: {}", e);
            return Err(e.into());
        }
    }
    Ok(())
}
