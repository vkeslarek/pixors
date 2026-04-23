use crate::error::Error;
use crate::server::event_bus::EngineEvent;
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

/// Request body for creating a tab.
#[derive(Debug, Deserialize)]
struct CreateTabRequest {
    /// Optional tile size (default: 256).
    tile_size: Option<u32>,
}

/// Response for creating a tab.
#[derive(Debug, Serialize)]
struct CreateTabResponse {
    tab_id: Uuid,
}

/// Request body for opening a file in a tab.
#[derive(Debug, Deserialize)]
struct OpenFileRequest {
    path: String,
}

/// Response for opening a file.
#[derive(Debug, Serialize)]
struct OpenFileResponse {
    width: u32,
    height: u32,
    tile_count: usize,
}

/// Response for tab info.
#[derive(Debug, Serialize)]
struct TabInfoResponse {
    tab_id: Uuid,
    has_image: bool,
    width: Option<u32>,
    height: Option<u32>,
}

/// Query parameters for WebSocket connection.
#[derive(Debug, Deserialize)]
struct WebSocketQuery {
    tab_id: Option<Uuid>,
}

/// Start the WebSocket server on the given address.
pub async fn start_server(addr: &str) -> Result<(), Error> {
    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        // REST API for tabs
        .route("/api/tabs", post(create_tab_handler).get(list_tabs_handler))
        .route("/api/tabs/:tab_id", get(get_tab_handler).delete(delete_tab_handler))
        .route("/api/tabs/:tab_id/open", post(open_file_handler))
        .route("/api/state", get(get_state_handler))
        // WebSocket (per-tab connection)
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

/// Handler for creating a new tab.
async fn create_tab_handler(
    State(state): State<Arc<AppState>>,
    Json(_request): Json<CreateTabRequest>,
) -> impl IntoResponse {
    let tab_id = state.create_tab().await;

    // Broadcast tab creation event
    tracing::info!("Broadcasting TabCreated for tab {}", tab_id);
    state.event_bus().broadcast(EngineEvent::TabCreated {
        tab_id,
        name: "New Tab".to_string(),
    }).await;

    (StatusCode::CREATED, Json(CreateTabResponse { tab_id }))
}

/// Handler for listing all tabs.
async fn list_tabs_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let tabs = state.list_tabs().await;
    
    (StatusCode::OK, Json(tabs))
}

/// Handler for getting tab info.
async fn get_tab_handler(
    State(state): State<Arc<AppState>>,
    Path(tab_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.image_info(&tab_id).await {
        Some((width, height)) => (
            StatusCode::OK,
            Json(TabInfoResponse {
                tab_id,
                has_image: true,
                width: Some(width),
                height: Some(height),
            }),
        ),
        None => (
            StatusCode::OK,
            Json(TabInfoResponse {
                tab_id,
                has_image: false,
                width: None,
                height: None,
            }),
        ),
    }
}

/// Handler for deleting a tab.
async fn delete_tab_handler(
    State(state): State<Arc<AppState>>,
    Path(tab_id): Path<Uuid>,
) -> impl IntoResponse {
    if state.delete_tab(&tab_id).await {
        // Broadcast tab closed event
        state.event_bus().broadcast(EngineEvent::TabClosed { tab_id }).await;
        (StatusCode::NO_CONTENT, ())
    } else {
        (StatusCode::NOT_FOUND, ())
    }
}

/// Handler for opening a file in a tab.
async fn open_file_handler(
    State(state): State<Arc<AppState>>,
    Path(tab_id): Path<Uuid>,
    Json(request): Json<OpenFileRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    match state.open_image(&tab_id, &request.path).await {
        Ok(()) => {
            // Get tile count from tile grid
            let tile_grid = state.tile_grid(&tab_id).await;
            let tile_count = tile_grid.map(|g| g.tile_count()).unwrap_or(0);
            
            let (width, height) = state.image_info(&tab_id).await.unwrap_or((0, 0));
            
            Ok((
                StatusCode::OK,
                Json(OpenFileResponse {
                    width,
                    height,
                    tile_count,
                }),
            ))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        )),
    }
}

/// Handler for getting full application state (for MCP).
async fn get_state_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let tabs = state.list_tabs().await;
    let tab_infos = futures::future::join_all(tabs.iter().map(|tab_id| {
        let state = state.clone();
        async move {
            let info = state.image_info(tab_id).await;
            (tab_id, info)
        }
    })).await;
    
    let result: Vec<_> = tab_infos.into_iter().map(|(tab_id, info)| {
        match info {
            Some((width, height)) => serde_json::json!({
                "tab_id": tab_id,
                "has_image": true,
                "width": width,
                "height": height
            }),
            None => serde_json::json!({
                "tab_id": tab_id,
                "has_image": false
            }),
        }
    }).collect();
    
    (StatusCode::OK, Json(result))
}

/// WebSocket connection handler with tab support.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WebSocketQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_connection(socket, state, query.tab_id))
}
