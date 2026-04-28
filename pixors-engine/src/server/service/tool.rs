use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCommand {
    SelectTool { tool: String },
    GetToolState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolEvent {
    ToolChanged { tool: String },
    ToolState { tool: String },
}

#[derive(Debug)]
pub struct ToolService {
    current_tool: RwLock<String>,
}

impl Default for ToolService {
    fn default() -> Self {
        Self { current_tool: RwLock::new("pan".into()) }
    }
}

#[async_trait]
impl Service for ToolService {
    type Command = ToolCommand;
    type Event = ToolEvent;

    async fn handle_command(&self, cmd: ToolCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        match cmd {
            ToolCommand::SelectTool { tool } => self.handle_select_tool(tool, state, ctx).await,
            ToolCommand::GetToolState => self.handle_get_tool_state(state, ctx).await,
        }
    }
}

impl ToolService {
    pub fn new() -> Self {
        Self::default()
    }

    async fn handle_select_tool(&self, tool: String, _state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        *self.current_tool.write().await = tool.clone();
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tool(ToolEvent::ToolChanged { tool }),
        );
    }

    async fn handle_get_tool_state(&self, _state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;
        let tool = self.current_tool.read().await.clone();
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tool(ToolEvent::ToolState { tool }),
        );
    }
}
