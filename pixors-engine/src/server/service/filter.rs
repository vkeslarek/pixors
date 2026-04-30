use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::server::app::AppState;
use crate::server::event_bus::EngineEvent;
use crate::server::service::Service;
use crate::server::ws::types::{send_session_event, ConnectionContext};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterCommand {
    ApplyGaussianBlur { tab_id: Uuid, radius: u32 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterEvent {
    FilterDone { job_id: Uuid },
    FilterFailed { job_id: Uuid, error: String },
}

#[derive(Debug)]
pub struct FilterService;

impl FilterService {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Service for FilterService {
    type Command = FilterCommand;
    type Event = FilterEvent;

    async fn handle_command(&self, cmd: FilterCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        tracing::debug!("[FilterService] handle_command: {:?}", cmd);
        match cmd {
            FilterCommand::ApplyGaussianBlur { tab_id, radius } => {
                self.apply_blur(tab_id, radius, state, ctx).await;
            }
        }
    }
}

impl FilterService {
    async fn apply_blur(&self, tab_id: Uuid, radius: u32, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        tracing::info!("[Filter] apply_blur tab={} radius={}", tab_id, radius);
        let job_id = Uuid::new_v4();

        let tab = state.session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.tabs.remove(&tab_id))
            .await.flatten();

        let Some(mut tab) = tab else {
            tracing::warn!("[Filter] tab {} not found", tab_id);
            send_session_event(&ctx.frame_tx, &EngineEvent::Filter(FilterEvent::FilterFailed {
                job_id, error: "Tab not found".into(),
            }));
            return;
        };

        if tab.image.is_none() {
            tracing::warn!("[Filter] no image loaded in tab {}", tab_id);
            send_session_event(&ctx.frame_tx, &EngineEvent::Filter(FilterEvent::FilterFailed {
                job_id, error: "No image loaded".into(),
            }));
            state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.tabs.insert(tab_id, tab)).await;
            return;
        }

        tracing::info!("[Filter] running gaussian blur radius={}...", radius);
        let (result, tab) = tokio::task::spawn_blocking(move || {
            let r = tab.image.as_mut().unwrap().apply_gaussian_blur(radius);
            (r, tab)
        }).await.expect("blur task panicked");
        let (iw, ih) = (tab.image.as_ref().map(|w| w.doc_width).unwrap_or(0),
                         tab.image.as_ref().map(|w| w.doc_height).unwrap_or(0));
        tracing::info!("[Filter] blur result: {:?}", result.as_ref().map(|_| "ok").map_err(|e| format!("{}", e)));

        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.tabs.insert(tab_id, tab)).await;

        match result {
            Ok(()) => {
                send_session_event(&ctx.frame_tx, &EngineEvent::Tab(
                    crate::server::service::tab::TabEvent::TilesDirty {
                        tab_id,
                        regions: vec![crate::server::service::tab::Rect { x: 0, y: 0, width: iw, height: ih }],
                    }
                ));
                send_session_event(&ctx.frame_tx, &EngineEvent::Filter(FilterEvent::FilterDone { job_id }));
            }
            Err(e) => {
                send_session_event(&ctx.frame_tx, &EngineEvent::Filter(FilterEvent::FilterFailed {
                    job_id, error: format!("{}", e),
                }));
            }
        }
    }
}
