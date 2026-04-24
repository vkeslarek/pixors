use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::app::AppState;
use crate::server::ws::types::ConnectionContext;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemEvent {
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemCommand {
    GetState,
    Close,
}

#[derive(Debug, Default)]
pub struct SystemService;

impl SystemService {
    pub fn new() -> Self {
        Self
    }

    pub async fn handle_command(
        &self,
        cmd: SystemCommand,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            SystemCommand::GetState => {
                self.handle_get_state(state).await;
            }
            SystemCommand::Close => {
                ctx.close_requested = true;
            }
        }
    }

    async fn handle_get_state(&self, state: &Arc<AppState>) {
        use crate::pixel::PixelFormat;
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::tab::TabEvent;

        let tabs = state.tab_service.list_tabs().await;
        for tab_id in tabs.iter() {
            state
                .event_bus
                .broadcast(EngineEvent::Tab(TabEvent::TabCreated {
                    tab_id: *tab_id,
                    name: "Tab".to_string(),
                }))
                .await;

            if let Some((width, height)) = state.tab_service.image_info(tab_id).await {
                state
                    .event_bus
                    .broadcast(EngineEvent::Tab(TabEvent::ImageLoaded {
                        tab_id: *tab_id,
                        width,
                        height,
                        format: PixelFormat::Rgba8,
                    }))
                    .await;
            }
        }

        if let Some(first_tab) = tabs.first() {
            state
                .event_bus
                .broadcast(EngineEvent::Tab(TabEvent::TabActivated {
                    tab_id: *first_tab,
                }))
                .await;
        }
    }
}
