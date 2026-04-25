use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use async_trait::async_trait;
use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::session::SessionStatus;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Serialize)]
pub struct SessionTab {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    pub has_image: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    State {
        session_id: Uuid,
        tabs: Vec<SessionTab>,
        active_tab_id: Option<Uuid>,
        status: SessionStatus,
    },
    Heartbeat,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionCommand {
    GetSessionState,
    Heartbeat,
}

#[derive(Debug)]
pub struct SessionService;

#[async_trait]
impl Service for SessionService {
    type Command = SessionCommand;
    type Event = SessionEvent;

    async fn handle_command(&self, cmd: SessionCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        self.handle_command_impl(cmd, state, ctx).await
    }
}

impl SessionService {
    pub fn new() -> Self {
        Self
    }

    async fn handle_command_impl(
        &self,
        cmd: SessionCommand,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            SessionCommand::GetSessionState => self.handle_get_session_state(state, ctx).await,
            SessionCommand::Heartbeat => self.handle_heartbeat(state, ctx).await,
        }
    }

    async fn handle_get_session_state(&self, state: &Arc<AppState>, ctx: &ConnectionContext) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        state.session_manager.with_tab_session(&ctx.session_id, |ts| {
            let tabs = ts.tabs.values().map(|t| SessionTab {
                id: t.id,
                name: t.name.clone(),
                created_at: t.created_at,
                has_image: t.source.is_some(),
                width: t.width,
                height: t.height,
            }).collect();
            send_session_event(
                &ctx.frame_tx,
                &EngineEvent::Session(SessionEvent::State {
                    session_id: ctx.session_id,
                    tabs,
                    active_tab_id: ts.active_tab_id,
                    status: SessionStatus::Connected,
                }),
            );
        }).await;
    }

    async fn handle_heartbeat(&self, state: &Arc<AppState>, ctx: &ConnectionContext) {
        state.session_manager.update_activity(&ctx.session_id).await;
    }
}
