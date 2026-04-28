//! Layer mutation service — owns layer visibility, opacity, and offset operations.
//!
//! Path: Session → Tab → layers[find(layer_id)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

/// Commands handled by the LayerService.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum LayerCommand {
    SetVisible {
        tab_id: Uuid,
        layer_id: Uuid,
        visible: bool,
    },
    SetOpacity {
        tab_id: Uuid,
        layer_id: Uuid,
        opacity: f32,
    },
    SetOffset {
        tab_id: Uuid,
        layer_id: Uuid,
        x: i32,
        y: i32,
    },
}

/// Events emitted by the LayerService.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerEvent {
    LayerChanged {
        tab_id: Uuid,
        layer_id: Uuid,
        field: String,
        composition_sig: u64,
    },
    DocSizeChanged {
        tab_id: Uuid,
        width: u32,
        height: u32,
    },
}

/// Manages layer-level mutations within a tab.
#[derive(Debug)]
pub struct LayerService;

impl LayerService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Service for LayerService {
    type Command = LayerCommand;
    type Event = LayerEvent;

    async fn handle_command(
        &self,
        cmd: Self::Command,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            LayerCommand::SetVisible { tab_id, layer_id, visible } => {
                self.handle_layer_set_visible(tab_id, layer_id, visible, state, ctx).await;
            }
            LayerCommand::SetOpacity { tab_id, layer_id, opacity } => {
                self.handle_layer_set_opacity(tab_id, layer_id, opacity, state, ctx).await;
            }
            LayerCommand::SetOffset { tab_id, layer_id, x, y } => {
                self.handle_layer_set_offset(tab_id, layer_id, x, y, state, ctx).await;
            }
        }
    }
}

impl LayerService {
    async fn handle_layer_set_visible(
        &self,
        tab_id: Uuid,
        layer_id: Uuid,
        visible: bool,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::layer::LayerEvent;
        use crate::server::ws::types::send_session_event;

        let result = state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| {
                let tab = ts.tabs.get_mut(&tab_id)?;
                let layer = tab.layers.iter_mut().find(|l| l.id == layer_id)?;
                let (prev_w, prev_h) = (tab.doc_width, tab.doc_height);
                layer.visible = visible;
                tab.recompute_doc_bounds();
                let changed = tab.doc_width != prev_w || tab.doc_height != prev_h;
                Some((
                    tab.composition_sig(),
                    prev_w,
                    prev_h,
                    tab.doc_width,
                    tab.doc_height,
                    changed,
                ))
            })
            .await
            .flatten();

        if let Some((sig, _pw, _ph, w, h, changed)) = result {
            send_session_event(
                &ctx.frame_tx,
                &EngineEvent::Layer(LayerEvent::LayerChanged {
                    tab_id,
                    layer_id,
                    field: "visible".into(),
                    composition_sig: sig,
                }),
            );
            if changed {
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::Layer(LayerEvent::DocSizeChanged { tab_id, width: w, height: h }),
                );
            }
        }
    }

    async fn handle_layer_set_opacity(
        &self,
        tab_id: Uuid,
        layer_id: Uuid,
        opacity: f32,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::layer::LayerEvent;
        use crate::server::ws::types::send_session_event;

        let sig = state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| {
                let tab = ts.tabs.get_mut(&tab_id)?;
                let layer = tab.layers.iter_mut().find(|l| l.id == layer_id)?;
                layer.opacity = opacity.clamp(0.0, 1.0);
                Some(tab.composition_sig())
            })
            .await
            .flatten();

        if let Some(s) = sig {
            send_session_event(
                &ctx.frame_tx,
                &EngineEvent::Layer(LayerEvent::LayerChanged {
                    tab_id,
                    layer_id,
                    field: "opacity".into(),
                    composition_sig: s,
                }),
            );
        }
    }

    async fn handle_layer_set_offset(
        &self,
        tab_id: Uuid,
        layer_id: Uuid,
        x: i32,
        y: i32,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::layer::LayerEvent;
        use crate::server::ws::types::send_session_event;

        let result = state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| {
                let tab = ts.tabs.get_mut(&tab_id)?;
                let layer = tab.layers.iter_mut().find(|l| l.id == layer_id)?;
                let (prev_w, prev_h) = (tab.doc_width, tab.doc_height);
                layer.offset = (x, y);
                tab.recompute_doc_bounds();
                let changed = tab.doc_width != prev_w || tab.doc_height != prev_h;
                Some((
                    tab.composition_sig(),
                    prev_w,
                    prev_h,
                    tab.doc_width,
                    tab.doc_height,
                    changed,
                ))
            })
            .await
            .flatten();

        if let Some((sig, _pw, _ph, w, h, changed)) = result {
            send_session_event(
                &ctx.frame_tx,
                &EngineEvent::Layer(LayerEvent::LayerChanged {
                    tab_id,
                    layer_id,
                    field: "offset".into(),
                    composition_sig: sig,
                }),
            );
            if changed {
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::Layer(LayerEvent::DocSizeChanged { tab_id, width: w, height: h }),
                );
            }
        }
    }
}
