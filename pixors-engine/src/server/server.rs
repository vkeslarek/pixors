use crate::config::Config;
use crate::error::Error;
use crate::server::app::AppState;
use crate::server::session::{heartbeat_broadcast_task, session_cleanup_task};
use crate::server::ws::handle_connection;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderValue;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{AllowMethods, AllowOrigin, Any, CorsLayer};
use uuid::Uuid;

fn build_router() -> (Router, Arc<AppState>) {
    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_methods(AllowMethods::any())
        .allow_origin(AllowOrigin::exact(HeaderValue::from_static("localhost")))
        .allow_headers(Any);

    let router = Router::new()
        .route("/ws", get(websocket_handler))
        .layer(cors)
        .with_state(state.clone());

    tokio::spawn(session_cleanup_task(state.clone()));
    tokio::spawn(heartbeat_broadcast_task(state.clone()));

    (router, state)
}

pub fn start_server_bg(cfg: Config) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            let (app, _state) = build_router();
            let addr = format!("127.0.0.1:{}", cfg.port);
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("failed to bind server address");
            eprintln!("Engine server on {}", addr);
            axum::serve(listener, app).await.expect("engine server crashed");
        });
    });
}

pub async fn start_server(cfg: Config) -> Result<(), Error> {
    let (app, _state) = build_router();

    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::AddrInUse, e)))?;

    tracing::info!("Server listening on {}", addr);
    tracing::debug!("Starting axum server...");

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("axum::serve failed: {}", e);
        return Err(Error::Io(std::io::Error::other(e)));
    }

    tracing::info!("Server stopped");
    Ok(())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let session_id = params
        .get("session_id")
        .and_then(|s| Uuid::parse_str(s).ok());
    ws.on_upgrade(move |socket| handle_connection(socket, state, session_id))
}
