use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::server::app::AppState;
use crate::server::event_bus::EngineEvent;
use crate::server::ws::types::{ClientFrame, ConnectionContext, MSG_TILE, MSG_TILES_COMPLETE};
use crate::image::TileCoord;

/// Commands handled by the ViewportService.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewportCommand {
    ViewportUpdate {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        zoom: f32,
    },
    RequestTiles {
        tab_id: Uuid,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        zoom: f32,
    },
}

/// Events emitted by the ViewportService.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewportEvent {
    ViewportUpdated {
        tab_id: Uuid,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    },
    MipLevelReady {
        tab_id: Uuid,
        level: u32,
        width: u32,
        height: u32,
    },
    TileInvalidated {
        tab_id: Uuid,
        coords: Vec<TileCoord>,
    },
}

/// Represents the physical state of the client's viewport.
#[derive(Debug, Clone)]
pub struct ViewportState {
    pub width: u32,
    pub height: u32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// Service responsible for tracking client viewports.
#[derive(Debug, Default)]
pub struct ViewportService {
    viewports: RwLock<HashMap<Uuid, ViewportState>>,
}

impl ViewportService {
    pub fn new() -> Self {
        Self {
            viewports: RwLock::new(HashMap::new()),
        }
    }

    /// Registers or updates a viewport state for a given tab.
    pub async fn update_viewport(&self, tab_id: &Uuid, state: ViewportState) {
        let mut viewports = self.viewports.write().await;
        viewports.insert(*tab_id, state);
    }

    /// Retrieves the current viewport state for a given tab.
    pub async fn get_viewport(&self, tab_id: &Uuid) -> Option<ViewportState> {
        let viewports = self.viewports.read().await;
        viewports.get(tab_id).cloned()
    }

    /// Removes a viewport state when a tab is closed.
    #[allow(dead_code)]
    pub async fn remove_viewport(&self, tab_id: &Uuid) {
        let mut viewports = self.viewports.write().await;
        viewports.remove(tab_id);
    }

    /// Handles a `ViewportCommand`, updating state and streaming tiles.
    pub async fn handle_command(
        &self,
        cmd: ViewportCommand,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            ViewportCommand::ViewportUpdate { x, y, w, h, zoom } => {
                let tab_id = match state.tab_service.active_tab().await {
                    Some(id) => id,
                    None => return,
                };

                let vp_state = ViewportState {
                    width: w as u32,
                    height: h as u32,
                    zoom,
                    pan_x: x,
                    pan_y: y,
                };
                self.update_viewport(&tab_id, vp_state).await;

                state
                    .event_bus
                    .broadcast(EngineEvent::Viewport(ViewportEvent::ViewportUpdated {
                        tab_id,
                        zoom,
                        pan_x: x,
                        pan_y: y,
                    }))
                    .await;

                // ViewportUpdate apenas salva estado — frontend envia RequestTiles explicitamente
            }
            ViewportCommand::RequestTiles { tab_id, x, y, w, h, zoom } => {
                let vp_state = ViewportState {
                    width: w as u32,
                    height: h as u32,
                    zoom,
                    pan_x: x,
                    pan_y: y,
                };
                self.update_viewport(&tab_id, vp_state.clone()).await;

                // Spawn tile streaming — reader stays responsive.
                let frame_tx = ctx.frame_tx.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    stream_tiles_for_tab(tab_id, frame_tx, state, Some(vp_state)).await;
                });
            }
        }
    }
}

/// Fetches and sends visible tiles for a tab through the writer channel.
/// Runs as a spawned background task — does not block the reader loop.
/// Fetches tiles concurrently (up to 8 in flight) for reduced latency.
pub(crate) async fn stream_tiles_for_tab(
    tab_id: Uuid,
    frame_tx: mpsc::UnboundedSender<ClientFrame>,
    state: Arc<AppState>,
    vp_state: Option<ViewportState>,
) {
    let vp_state = match vp_state {
        Some(s) => s,
        None => return,
    };

    let zoom = vp_state.zoom.max(0.0001);
    let viewport_width = vp_state.width as f32 / zoom;
    let viewport_height = vp_state.height as f32 / zoom;
    let mip_level = compute_mip_level(zoom) as u32;

    // Ensure MIP level is generated before fetching tiles
    if let Err(e) = state.tab_service.ensure_mip_level(&tab_id, zoom).await {
        tracing::error!("Failed to ensure MIP level for zoom {}: {}", zoom, e);
        return;
    }

    // Broadcast MipLevelReady so the client knows which resolution is available
    if let Some((width, height)) = state.tab_service.image_info(&tab_id).await {
        state
            .event_bus
            .broadcast(EngineEvent::Viewport(ViewportEvent::MipLevelReady {
                tab_id,
                level: mip_level,
                width,
                height,
            }))
            .await;
    }

    let tile_grid = match state.tab_service.tile_grid(&tab_id).await {
        Some(g) => g,
        None => return,
    };

    let visible_tiles = tile_grid.tiles_in_viewport(
        mip_level,
        vp_state.pan_x,
        vp_state.pan_y,
        viewport_width,
        viewport_height,
    );

    if visible_tiles.is_empty() {
        let _ = frame_tx.send(ClientFrame::empty(MSG_TILES_COMPLETE));
        return;
    }

    let semaphore = Arc::new(Semaphore::new(8));
    let mut join_set = JoinSet::new();

    for tile_ref in &visible_tiles {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let state = state.clone();
        let frame_tx = frame_tx.clone();
        let tile = *tile_ref;
        join_set.spawn(async move {
            let _permit = permit;

            let rgba8 = match state.tab_service.get_tile_rgba8(&tab_id, tile, mip_level as usize).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Failed to get tile data: {}", e);
                    return;
                }
            };

            let mut pixel_data = rgba8;
            let payload = encode_tile_payload(tab_id, &tile, mip_level, &mut pixel_data);

            if frame_tx.send(ClientFrame::new(MSG_TILE, payload)).is_err() {
                tracing::error!("Writer task closed");
            }
        });
    }

    while let Some(result) = join_set.join_next().await {
        if let Err(e) = result {
            tracing::error!("Tile streaming task panicked: {}", e);
        }
    }

    let _ = frame_tx.send(ClientFrame::empty(MSG_TILES_COMPLETE));
}

/// Computes the MIP level to use for a given zoom factor.
/// Returns 0 for zoom >= 1.0 (base level), otherwise ceil(log2(1/zoom)).
fn compute_mip_level(zoom: f32) -> usize {
    if zoom >= 1.0 {
        return 0;
    }
    let level = (1.0 / zoom).log2().ceil() as usize;
    level
}

/// Build binary tile message: 36-byte header + RGBA8 pixel data.
///
/// Header format (little-endian):
/// [4B px][4B py][4B width][4B height][4B mip_level][16B tab_id UUID]
fn encode_tile_payload(tab_id: Uuid, tile: &TileCoord, mip_level: u32, pixels: &mut Vec<u8>) -> Vec<u8> {
    let mut buf = Vec::with_capacity(36 + pixels.len());
    buf.extend_from_slice(&tile.px.to_le_bytes());
    buf.extend_from_slice(&tile.py.to_le_bytes());
    buf.extend_from_slice(&tile.width.to_le_bytes());
    buf.extend_from_slice(&tile.height.to_le_bytes());
    buf.extend_from_slice(&mip_level.to_le_bytes());
    buf.extend_from_slice(tab_id.as_bytes());
    buf.append(pixels);
    buf
}
