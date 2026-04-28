use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemEvent {
    Error { message: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemCommand {
    Close,
}

#[derive(Debug, Default)]
pub struct SystemService;

#[async_trait]
impl Service for SystemService {
    type Command = SystemCommand;
    type Event = SystemEvent;

    async fn handle_command(&self, cmd: SystemCommand, _state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        match cmd {
            SystemCommand::Close => self.handle_close(ctx),
        }
    }
}

impl SystemService {
    pub fn new() -> Self { Self }

    fn handle_close(&self, ctx: &mut ConnectionContext) {
        ctx.close_requested = true;
    }
}
