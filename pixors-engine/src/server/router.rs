use crate::error::Error;
use crate::server::state::AppState;
use crate::server::ws::handle_connection;
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{StatusCode, Method},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

/// Request body for creating a session.
#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    /// Optional tile size (default: 256).
    tile_size: Option<u32>,
}

/// Response for creating a session.
#[derive(Debug, Serialize)]
struct CreateSessionResponse {
    session_id: Uuid,
}

/// Request body for opening a file.
#[derive(Debug, Deserialize)]
struct OpenFileRequest {
    session_id: Uuid,
    path: String,
}

/// Response for opening a file.
#[derive(Debug, Serialize)]
struct OpenFileResponse {
    width: u32,
    height: u32,
}

/// Query parameters for WebSocket connection.
#[derive(Debug, Deserialize)]
struct WebSocketQuery {
    session_id: Option<Uuid>,
}

/// Start the WebSocket server on the given address.
pub async fn start_server(addr: &str) -> Result<(), Error> {
    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        // REST API
        .route("/api/session", post(create_session_handler))
        .route("/api/session/:session_id", get(get_session_handler).delete(delete_session_handler))
        .route("/api/file/open", post(open_file_handler))
        // WebSocket
        .route("/ws", get(websocket_handler))
        .layer(cors)
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

/// Handler for creating a new session.
async fn create_session_handler(
    State(state): State<Arc<AppState>>,
    Json(_request): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let session_id = state.create_session().await;
    
    (StatusCode::CREATED, Json(CreateSessionResponse { session_id }))
}

/// Handler for getting session info.
async fn get_session_handler(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.image_info(&session_id).await {
        Some((width, height)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": session_id,
                "has_image": true,
                "width": width,
                "height": height
            })),
        ),
        None => (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": session_id,
                "has_image": false
            })),
        ),
    }
}

/// Handler for deleting a session.
async fn delete_session_handler(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> impl IntoResponse {
    if state.delete_session(&session_id).await {
        (StatusCode::NO_CONTENT, ())
    } else {
        (StatusCode::NOT_FOUND, ())
    }
}

/// Handler for opening a file in a session.
async fn open_file_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<OpenFileRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.load_image(&request.session_id, &request.path).await {
        Ok((width, height)) => Ok((
            StatusCode::OK,
            Json(OpenFileResponse { width, height }),
        )),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )),
    }
}

/// WebSocket connection handler with session support.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WebSocketQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_connection(socket, state, query.session_id))
}
