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
    GetState,
    Close,
}

#[derive(Debug, Default)]
pub struct SystemService;

#[async_trait]
impl Service for SystemService {
    type Command = SystemCommand;
    type Event = SystemEvent;

    async fn handle_command(&self, cmd: SystemCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        match cmd {
            SystemCommand::GetState => self.handle_get_state(state, ctx).await,
            SystemCommand::Close => self.handle_close(ctx),
        }
    }
}

impl SystemService {
    pub fn new() -> Self { Self }

    async fn handle_get_state(&self, state: &Arc<AppState>, ctx: &ConnectionContext) {
        state.session_manager.with_tab_session(&ctx.session_id, |ts| {
            for t in ts.tabs.values() {
                tracing::debug!("Tab: {} ({})", t.name, t.id);
            }
        }).await;
    }

    fn handle_close(&self, ctx: &mut ConnectionContext) {
        ctx.close_requested = true;
    }
}
