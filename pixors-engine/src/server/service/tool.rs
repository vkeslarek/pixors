use serde::{Deserialize, Serialize};

/// Commands handled by the ToolService.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCommand {
    SelectTool {
        tool: String,
    },
}

/// Events emitted by the ToolService.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolEvent {
    ToolChanged {
        tool: String,
    },
}

/// Service responsible for tracking active tool state.
#[derive(Debug, Default)]
pub struct ToolService;

impl ToolService {
    pub fn new() -> Self {
        Self
    }

    /// Handles a `ToolCommand`, broadcasting events.
    pub async fn handle_command(
        &self,
        cmd: ToolCommand,
        state: &crate::server::app::AppState,
        _ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;

        match cmd {
            ToolCommand::SelectTool { tool } => {
                state.event_bus.broadcast(
                    EngineEvent::Tool(ToolEvent::ToolChanged { tool }),
                ).await;
            }
        }
    }
}
