use serde::{Deserialize, Serialize};
use std::sync::Arc;

use async_trait::async_trait;
use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    Heartbeat,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionCommand {
    Heartbeat,
}

#[derive(Debug)]
pub struct SessionService;

#[async_trait]
impl Service for SessionService {
    type Command = SessionCommand;
    type Event = SessionEvent;

    async fn handle_command(&self, cmd: SessionCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        match cmd {
            SessionCommand::Heartbeat => self.handle_heartbeat(state, ctx).await,
        }
    }
}

impl SessionService {
    pub fn new() -> Self { Self }

    async fn handle_heartbeat(&self, state: &Arc<AppState>, ctx: &ConnectionContext) {
        state.session_manager.update_activity(&ctx.session_id).await;
    }
}
