use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCommand {
    SelectTool { tool: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolEvent {
    ToolChanged { tool: String },
}

#[derive(Debug, Default)]
pub struct ToolService;

#[async_trait]
impl Service for ToolService {
    type Command = ToolCommand;
    type Event = ToolEvent;

    async fn handle_command(&self, cmd: ToolCommand, state: &Arc<AppState>, _ctx: &mut ConnectionContext) {
        match cmd {
            ToolCommand::SelectTool { tool } => self.handle_select_tool(tool, state).await,
        }
    }
}

impl ToolService {
    pub fn new() -> Self { Self }

    async fn handle_select_tool(&self, tool: String, state: &Arc<AppState>) {
        use crate::server::event_bus::EngineEvent;
        state.event_bus.broadcast(
            EngineEvent::Tool(ToolEvent::ToolChanged { tool }),
        ).await;
    }
}
