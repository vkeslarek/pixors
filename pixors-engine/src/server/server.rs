use crate::error::Error;
use crate::server::app::AppState;
use crate::server::ws::handle_connection;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Start the HTTP + WebSocket server on the given address.
pub async fn start_server(addr: &str) -> Result<(), Error> {
    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_methods(tower_http::cors::AllowMethods::any())
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        Error::Io(std::io::Error::new(std::io::ErrorKind::AddrInUse, e))
    })?;

    tracing::info!("Server listening on {}", addr);
    tracing::debug!("Starting axum server...");

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("axum::serve failed: {}", e);
        return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)));
    }

    tracing::info!("Server stopped");
    Ok(())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_connection(socket, state))
}
