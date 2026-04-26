use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::task::JoinSet;
use uuid::Uuid;

use async_trait::async_trait;
use crate::debug_stopwatch;
use crate::server::app::AppState;
use crate::server::event_bus::EngineEvent;
use crate::server::service::Service;
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
    request_gen: RwLock<HashMap<Uuid, Arc<AtomicU64>>>,
}

#[async_trait]
impl Service for ViewportService {
    type Command = ViewportCommand;
    type Event = ViewportEvent;

    async fn handle_command(&self, cmd: ViewportCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        self.handle_command_impl(cmd, state, ctx).await
    }
}

impl ViewportService {
    pub fn new() -> Self {
        Self {
            viewports: RwLock::new(HashMap::new()),
            request_gen: RwLock::new(HashMap::new()),
        }
    }

    /// Bump and return the new generation for a tab's tile request.
    pub(crate) async fn next_request_gen(&self, tab_id: &Uuid) -> (Arc<AtomicU64>, u64) {
        let mut map = self.request_gen.write().await;
        let counter = map.entry(*tab_id).or_insert_with(|| Arc::new(AtomicU64::new(0))).clone();
        let current = counter.fetch_add(1, Ordering::SeqCst) + 1;
        (counter, current)
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

    async fn handle_command_impl(
        &self,
        cmd: ViewportCommand,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        match cmd {
            ViewportCommand::ViewportUpdate { x, y, w, h, zoom } => self.handle_viewport_update(x, y, w, h, zoom, state, ctx).await,
            ViewportCommand::RequestTiles { tab_id, x, y, w, h, zoom } => self.handle_request_tiles(tab_id, x, y, w, h, zoom, state, ctx).await,
        }
    }

    async fn handle_viewport_update(
        &self,
        x: f32, y: f32, w: f32, h: f32, zoom: f32,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        let Some(tab_id) = state.session_manager.with_tab_session(&ctx.session_id, |ts| ts.active_tab_id).await.flatten() else { return };
        let vp_state = ViewportState {
            width: w as u32,
            height: h as u32,
            zoom,
            pan_x: x,
            pan_y: y,
        };
        self.update_viewport(&tab_id, vp_state).await;
        use crate::server::ws::types::send_session_event;
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Viewport(ViewportEvent::ViewportUpdated { tab_id, zoom, pan_x: x, pan_y: y }),
        );
    }

    async fn handle_request_tiles(
        &self,
        tab_id: Uuid, x: f32, y: f32, w: f32, h: f32, zoom: f32,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        tracing::trace!("request_tiles: tab={} zoom={:.3} x={:.0} y={:.0} w={:.0} h={:.0}", tab_id, zoom, x, y, w, h);
        let vp_state = ViewportState {
            width: w as u32,
            height: h as u32,
            zoom,
            pan_x: x,
            pan_y: y,
        };
        self.update_viewport(&tab_id, vp_state.clone()).await;
        let (gen_counter, my_gen) = self.next_request_gen(&tab_id).await;
        let frame_tx = ctx.frame_tx.clone();
        let session_id = ctx.session_id;
        let state = state.clone();
        tokio::spawn(async move {
            stream_tiles_for_tab(tab_id, session_id, frame_tx, state, Some(vp_state), gen_counter, my_gen).await;
        });
    }
}

/// Fetches and sends visible tiles for a tab through the writer channel.
/// Runs as a spawned background task — does not block the reader loop.
/// Fetches tiles concurrently (up to 8 in flight) for reduced latency.
/// Check if this request has been superseded by a newer one.
macro_rules! bail_if_stale {
    ($counter:expr, $gen:expr) => {
        if $counter.load(Ordering::SeqCst) != $gen {
            tracing::debug!("Stream cancelled (superseded by newer request)");
            return;
        }
    };
}


pub(crate) async fn stream_tiles_for_tab(
    tab_id: Uuid,
    session_id: Uuid,
    frame_tx: mpsc::UnboundedSender<ClientFrame>,
    state: Arc<AppState>,
    vp_state: Option<ViewportState>,
    gen_counter: Arc<AtomicU64>,
    my_gen: u64,
) {
    let vp_state = match vp_state {
        Some(s) => s,
        None => {
            tracing::warn!("stream_tiles: no viewport state for tab {}", tab_id);
            return;
        }
    };

    tracing::debug!("stream_tiles: start tab={} zoom={:.3}", tab_id, vp_state.zoom);

    bail_if_stale!(gen_counter, my_gen);

    let zoom = vp_state.zoom.max(0.0001);
    let viewport_width = vp_state.width as f32;
    let viewport_height = vp_state.height as f32;
    let desired_mip = crate::image::MipPyramid::level_for_zoom(zoom) as u32;
    tracing::debug!("stream_tiles_for_tab: tab={} zoom={:.3} desired_mip={}", tab_id, zoom, desired_mip);
    tracing::debug!("stream_tiles: desired_mip={}", desired_mip);

    let mut mip_level = 0u32;
    let mut need_sot_generation = false;
    if desired_mip > 0 {
        // Check display MIPs (RAM) — primary source for viewport streaming
        let (display_ready, sot_ready) = {
            let session_arc = match state.session_manager.get(&session_id).await {
                Some(s) => s,
                None => return,
            };
            let session = session_arc.read().await;
            let tab = match session.tab_session.tabs.get(&tab_id) {
                Some(t) => t,
                None => return,
            };
            (
                tab.is_display_mip_ready(desired_mip as usize),
                tab.is_mip_ready(desired_mip as usize),
            )
        };

        if display_ready {
            mip_level = desired_mip;
        } else {
            // Fallback: find highest display MIP level available
            let mut best_display = 0u32;
            for m in (1..desired_mip).rev() {
                let session_arc = match state.session_manager.get(&session_id).await {
                    Some(s) => s,
                    None => return,
                };
                let session = session_arc.read().await;
                let ready = session
                    .tab_session
                    .tabs
                    .get(&tab_id)
                    .map(|tab| tab.is_display_mip_ready(m as usize))
                    .unwrap_or(false);
                if ready {
                    best_display = m;
                    break;
                }
            }
            mip_level = best_display;
        }

        need_sot_generation = !sot_ready;
    }

    // Spawn background SOT MIP generation in the background if needed
    if need_sot_generation {
        let state_bg = state.clone();
        let tab_id_bg = tab_id;
        let session_id_bg = session_id;
        let frame_tx_bg = frame_tx.clone();
        tokio::spawn(async move {
            let Some(s) = state_bg.session_manager.get(&session_id_bg).await else { return };
            let mut tab = match s.write().await.tab_session.tabs.remove(&tab_id_bg) {
                Some(t) => t,
                None => return,
            };
            if let Err(e) = crate::server::service::tab::TabService::ensure_mip_level(&mut tab, zoom).await {
                tracing::error!("Background MIP generation failed: {}", e);
                if let Some(s) = state_bg.session_manager.get(&session_id_bg).await {
                    s.write().await.tab_session.tabs.insert(tab_id_bg, tab);
                }
                return;
            }
            let (width, height) = tab.image_info().unwrap_or((0, 0));
            if let Some(s) = state_bg.session_manager.get(&session_id_bg).await {
                s.write().await.tab_session.tabs.insert(tab_id_bg, tab);
            }
            tracing::debug!("Background MIP {} ready, notifying client", desired_mip);
            let event = EngineEvent::Viewport(
                ViewportEvent::MipLevelReady { tab_id: tab_id_bg, level: desired_mip, width, height }
            );
            use crate::server::ws::types::send_session_event;
            send_session_event(&frame_tx_bg, &event);
        });
    }

    // Get tile grid (read from session)
    let (tile_grid, mip_scale) = {
        let session_arc = match state.session_manager.get(&session_id).await {
            Some(s) => s,
            None => return,
        };
        let session = session_arc.read().await;
        let tab = match session.tab_session.tabs.get(&tab_id) {
            Some(t) => t,
            None => return,
        };
        let mip_scale = 0.5_f32.powi(mip_level as i32);
        let tile_grid = match tab.tile_grid_for_mip(mip_level as usize) {
            Some(g) => g,
            None => {
                tracing::warn!("No tile grid for mip_level={}, tab={}", mip_level, tab_id);
                return;
            }
        };
        (tile_grid, mip_scale)
    };

    let visible_tiles = {
        let _sw = debug_stopwatch!("stream_tiles:viewport_calc");
        tile_grid.tiles_in_viewport(
            mip_level,
            vp_state.pan_x * mip_scale,
            vp_state.pan_y * mip_scale,
            viewport_width * mip_scale,
            viewport_height * mip_scale,
        )
    };

    tracing::debug!("stream_tiles: sending {} tiles at mip={} (desired={})", visible_tiles.len(), mip_level, desired_mip);

    if visible_tiles.is_empty() {
        let _ = frame_tx.send(ClientFrame::empty(MSG_TILES_COMPLETE));
        tracing::debug!("stream_tiles: no visible tiles");
        return;
    }

    let semaphore = Arc::new(Semaphore::new(8));
    let mut join_set = JoinSet::new();
    let tiles_count = visible_tiles.len();

    {
        let _sw = debug_stopwatch!("stream_tiles:tile_fetching");
        for tile_ref in &visible_tiles {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let state = state.clone();
            let frame_tx = frame_tx.clone();
            let tile = *tile_ref;
            let tid = tab_id;
            let sid = session_id;
            let mip_lvl = mip_level;
            join_set.spawn(async move {
                let _permit = permit;

                let rgba8 = {
                    let session_arc = match state.session_manager.get(&sid).await {
                        Some(s) => s,
                        None => return,
                    };
                    let session = session_arc.read().await;
                    let tab = match session.tab_session.tabs.get(&tid) {
                        Some(t) => t,
                        None => return,
                    };
                    match tab.get_tile_rgba8(tile, mip_lvl as usize).await {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::error!("Failed to get tile data: {}", e);
                            return;
                        }
                    }
                };

                let payload = encode_tile_payload(tid, &tile, mip_lvl, &rgba8);

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
    }

    tracing::debug!("stream_tiles: done {} tiles", tiles_count);
    let _ = frame_tx.send(ClientFrame::empty(MSG_TILES_COMPLETE));
}

/// Build binary tile message: 36-byte header + RGBA8 pixel data.
///
/// Header format (little-endian):
/// [4B px][4B py][4B width][4B height][4B mip_level][16B tab_id UUID]
pub fn encode_tile_payload(tab_id: Uuid, tile: &TileCoord, mip_level: u32, pixels: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(36 + pixels.len());
    buf.extend_from_slice(&tile.px.to_le_bytes());
    buf.extend_from_slice(&tile.py.to_le_bytes());
    buf.extend_from_slice(&tile.width.to_le_bytes());
    buf.extend_from_slice(&tile.height.to_le_bytes());
    buf.extend_from_slice(&mip_level.to_le_bytes());
    buf.extend_from_slice(tab_id.as_bytes());
    buf.extend_from_slice(pixels);
    buf
}
