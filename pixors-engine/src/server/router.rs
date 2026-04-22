use crate::error::Error;
use crate::server::state::AppState;
use crate::server::ws::handle_connection;
use axum::{
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use std::sync::Arc;

/// Start the WebSocket server on the given address.
pub async fn start_server(addr: &str) -> Result<(), Error> {
    let state = Arc::new(AppState::default());

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        Error::Io(std::io::Error::new(std::io::ErrorKind::AddrInUse, e))
    })?;

    tracing::info!("WebSocket server listening on {}", addr);
    tracing::debug!("Starting axum server...");

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("axum::serve failed: {}", e);
        return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)));
    }
    
    tracing::info!("Server stopped");
    Ok(())
}

/// WebSocket connection handler.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_connection(socket, state))
}
