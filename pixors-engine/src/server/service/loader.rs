//! Image loading service — opens files, sets up the stream pipeline.
//!
//! Path: Session → Tab → pipeline setup

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::pixel::PixelFormat;
use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

/// Commands handled by the LoaderService.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoaderCommand {
    OpenFile {
        tab_id: Uuid,
        path: String,
    },
    OpenFileDialog {
        #[serde(default)]
        tab_id: Option<Uuid>,
    },
}

/// Events emitted by the LoaderService.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoaderEvent {
    ImageLoaded {
        tab_id: Uuid,
        width: u32,
        height: u32,
        format: PixelFormat,
        layer_count: usize,
    },
    ImageLoadProgress {
        tab_id: Uuid,
        percent: u8,
    },
}

/// Manages image loading (file open dialog + stream pipeline setup).
#[derive(Debug)]
pub struct LoaderService;

impl LoaderService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Service for LoaderService {
    type Command = LoaderCommand;
    type Event = LoaderEvent;

    async fn handle_command(
        &self,
        cmd: Self::Command,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            LoaderCommand::OpenFile { tab_id, path } => {
                self.handle_open_file(tab_id, &path, state, ctx).await;
            }
            LoaderCommand::OpenFileDialog { tab_id } => {
                self.handle_open_file_dialog(tab_id, state, ctx).await;
            }
        }
    }
}

impl LoaderService {
    async fn handle_open_file(
        &self,
        tab_id: Uuid,
        path: &str,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        use crate::debug_stopwatch;
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::loader::LoaderEvent;
        use crate::server::ws::types::send_session_event;

        let tab = state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.tabs.remove(&tab_id))
            .await
            .flatten();
        let Some(mut tab) = tab else {
            tracing::error!("Tab {} not found in session", tab_id);
            return;
        };

        let vp_svc = state.viewport_service.clone();
        let frame_tx_cb = ctx.frame_tx.clone();
        let tid = tab_id;
        let rt = tokio::runtime::Handle::current();
        let cb: Arc<dyn Fn(u32, crate::image::TileCoord, std::sync::Arc<Vec<u8>>) + Send + Sync> = Arc::new(move |mip, coord, data| {
            let vp_svc = vp_svc.clone();
            let frame_tx_cb = frame_tx_cb.clone();
            rt.spawn(async move {
                let vp_state = match vp_svc.get_viewport(&tid).await {
                    Some(s) => s,
                    None => return,
                };
                let zoom = vp_state.zoom.max(0.0001);
                let desired_mip = crate::image::MipPyramid::level_for_zoom(zoom) as u32;
                if mip != desired_mip { return; }

                let mip_scale = 0.5_f32.powi(mip as i32);
                let tx = coord.px as f32 * mip_scale;
                let ty = coord.py as f32 * mip_scale;
                let tw = coord.width as f32 * mip_scale;
                let th = coord.height as f32 * mip_scale;

                let vx = vp_state.pan_x * mip_scale;
                let vy = vp_state.pan_y * mip_scale;
                let vw = vp_state.width as f32 * mip_scale;
                let vh = vp_state.height as f32 * mip_scale;

                if tx < vx + vw && tx + tw > vx && ty < vy + vh && ty + th > vy {
                    let payload = crate::server::service::viewport::encode_tile_payload(tid, &coord, mip, &data);
                    let _ = frame_tx_cb.send(crate::server::ws::types::ClientFrame::new(crate::server::ws::types::MSG_TILE, payload));
                }
            });
        });

        {
            let _sw = debug_stopwatch!("OpenFile:open_image");
            match tab.open_image(path, tab_id, &ctx.frame_tx, Some(cb)).await {
                Ok(()) => tracing::debug!("open_image: done tab={}", tab_id),
                Err(e) => {
                    tracing::error!("Failed to load image: {}", e);
                    state
                        .session_manager
                        .with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab))
                        .await;
                    send_session_event(
                        &ctx.frame_tx,
                        &EngineEvent::Error {
                            message: format!("Failed to load image: {}", e),
                        },
                    );
                    return;
                }
            }
        }

        tracing::debug!("open_image: done tab={}", tab_id);
        let (width, height) = tab.image_info().unwrap_or((0, 0));
        let layer_count = tab.layers.len();
        state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab))
            .await;

        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Loader(LoaderEvent::ImageLoaded {
                tab_id,
                width,
                height,
                format: PixelFormat::Rgba8,
                layer_count,
            }),
        );
    }

    async fn handle_open_file_dialog(
        &self,
        tab_id: Option<Uuid>,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::tab::TabEvent;
        use crate::server::ws::types::send_session_event;

        let path_opt = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .add_filter(
                    "Image",
                    &["png", "tiff", "tif", "jpg", "jpeg", "exr", "hdr"],
                )
                .pick_file()
        })
        .await
        .unwrap_or(None);

        if let Some(path_buf) = path_opt
            && let Some(path_str) = path_buf.to_str()
        {
            let target_tab_id = match tab_id {
                Some(id) => id,
                None => {
                    let name = path_buf
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let tab = state.tab_service.create_tab_data(name.clone());
                    let new_id = tab.id;
                    state
                        .session_manager
                        .with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab))
                        .await;
                    send_session_event(
                        &ctx.frame_tx,
                        &EngineEvent::Tab(TabEvent::TabCreated { tab_id: new_id, name }),
                    );
                    state
                        .session_manager
                        .with_tab_session_mut(&ctx.session_id, |ts| ts.set_active(Some(new_id)))
                        .await;
                    send_session_event(
                        &ctx.frame_tx,
                        &EngineEvent::Tab(TabEvent::TabActivated { tab_id: new_id }),
                    );
                    new_id
                }
            };
            let cmd = LoaderCommand::OpenFile {
                tab_id: target_tab_id,
                path: path_str.to_string(),
            };
            Box::pin(self.handle_command(cmd, state, ctx)).await;
        }
    }
}
