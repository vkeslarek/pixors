//! Application composition root — AppState and command routing.

use crate::server::event_bus::{EngineCommand, EventBus};
use crate::server::service::Service;
use crate::server::service::session::SessionService;
use crate::server::service::system::SystemService;
use crate::server::service::tab::TabService;
use crate::server::service::tool::ToolService;
use crate::server::service::viewport::ViewportService;
use crate::server::session::SessionManager;
use crate::server::ws::types::ConnectionContext;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Global application state — a container of service Arc's.
/// Controllers and WebSocket handlers access fields directly.
#[derive(Debug, Clone)]
pub struct AppState {
    pub tab_service: Arc<TabService>,
    pub tool_service: Arc<ToolService>,
    pub viewport_service: Arc<ViewportService>,
    pub system_service: Arc<SystemService>,
    pub session_service: Arc<SessionService>,
    pub event_bus: Arc<EventBus>,
    pub session_manager: Arc<SessionManager>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            event_bus: EventBus::new(),
            tab_service: Arc::new(TabService::new(256)),
            tool_service: Arc::new(ToolService::new()),
            viewport_service: Arc::new(ViewportService::new()),
            system_service: Arc::new(SystemService::new()),
            session_service: Arc::new(SessionService::new()),
            session_manager: Arc::new(SessionManager::new()),
        }
    }

    /// Dispatch a decoded command to the correct service.
    /// Add new services here — this is the only place routing lives.
    pub async fn route_command(&self, cmd: EngineCommand, ctx: &mut ConnectionContext) {
        let state = Arc::new(self.clone());
        match cmd {
            EngineCommand::Session(c) => {
                self.session_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::Tab(c) => {
                self.tab_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::Viewport(c) => {
                self.viewport_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::Tool(c) => {
                self.tool_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::System(c) => {
                self.system_service.handle_command(c, &state, ctx).await;
            }
        }
    }
}
