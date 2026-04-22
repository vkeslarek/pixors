//! Pixors engine headless server.
//!
//! Starts a WebSocket server that accepts commands from the frontend
//! and streams image data to connected clients.

use pixors_engine::server::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let log_level = if cfg!(debug_assertions) {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .init();

    let addr = "127.0.0.1:8080";
    tracing::info!("Starting Pixors engine server on {}", addr);
    match start_server(addr).await {
        Ok(()) => tracing::info!("Server exited normally"),
        Err(e) => {
            tracing::error!("Server failed: {}", e);
            return Err(e.into());
        }
    }
    Ok(())
}