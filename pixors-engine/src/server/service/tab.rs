//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::color::ColorSpace;
use crate::composite::{self, CompositeRequest, LayerView};
use crate::convert::ColorConversion;
use crate::error::Error;
use crate::image::{BlendMode, MipPyramid, TileCoord, TileGrid};
use crate::pipeline::operation::color::ColorConvertOperation;
use crate::pipeline::sink::viewport::{Viewport, ViewportSink};
use crate::pixel::Rgba;
use crate::storage::writer::WorkingWriter;
use async_trait::async_trait;
use half::f16;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use crate::pipeline::sink::working::WorkingSink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

use crate::server::app::AppState;
use crate::server::service::Service;
use crate::server::ws::types::ConnectionContext;

/// Session-owned container for all tab state.
#[derive(Debug)]
pub struct TabSessionData {
    pub tabs: HashMap<Uuid, TabData>,
    pub active_tab_id: Option<Uuid>,
}

impl TabSessionData {
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            active_tab_id: None,
        }
    }

    pub fn add(&mut self, tab: TabData) {
        self.tabs.insert(tab.id, tab);
    }

    pub fn remove(&mut self, tab_id: &Uuid) -> Option<TabData> {
        if self.active_tab_id == Some(*tab_id) {
            self.active_tab_id = None;
        }
        self.tabs.remove(tab_id)
    }

    pub fn get(&self, tab_id: &Uuid) -> Option<&TabData> {
        self.tabs.get(tab_id)
    }

    pub fn set_active(&mut self, tab_id: Option<Uuid>) {
        self.active_tab_id = tab_id;
    }

    pub fn tab_ids(&self) -> impl Iterator<Item = &Uuid> {
        self.tabs.keys()
    }
}

/// Commands handled by the TabService.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TabCommand {
    CreateTab,
    CloseTab { tab_id: Uuid },
    ActivateTab { tab_id: Uuid },
    GetTabState,
    MarkTilesDirty { tab_id: Uuid, regions: Vec<Rect> },
}

/// Events emitted by the TabService.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TabEvent {
    TabCreated {
        tab_id: Uuid,
        name: String,
    },
    TabClosed {
        tab_id: Uuid,
    },
    TabActivated {
        tab_id: Uuid,
    },
    TabState {
        tabs: Vec<TabInfo>,
        active_tab_id: Option<Uuid>,
    },
    ImageClosed {
        tab_id: Uuid,
    },
    TilesDirty {
        tab_id: Uuid,
        regions: Vec<Rect>,
    },
}

/// Serializable tab info returned by GetTabState.
#[derive(Debug, Clone, Serialize)]
pub struct TabInfo {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    pub has_image: bool,
    pub width: u32,
    pub height: u32,
}

/// Per-layer tile storage and state bundle.
///
/// Each layer owns its own `TileStore`, `MipPyramid`, and `TileGrid`.
/// The compositor reads from all visible `LayerSlot`s to produce a
/// single blended display tile.
pub struct LayerSlot {
    pub id: Uuid,
    pub tile_store: Arc<WorkingWriter>,
    pub mip_pyramid: MipPyramid,
    pub mip_base_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub offset: (i32, i32),
    pub opacity: f32,
    pub visible: bool,
    pub blend_mode: BlendMode,
    pub viewport: Arc<Viewport>,
    pub disk_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LayerSlot {
    fn drop(&mut self) {
        if let Some(h) = self.disk_handle.take() {
            let _ = h.join();
        }
    }
}

/// State for a single image editing tab.
#[derive(Serialize)]
pub struct TabData {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    #[serde(skip)]
    pub has_image: bool,
    #[serde(skip)]
    pub color_conversion: Option<ColorConversion>,
    pub tile_size: u32,
    #[serde(skip)]
    pub layers: Vec<LayerSlot>,
    pub doc_width: u32,
    pub doc_height: u32,
    pub doc_origin: (i32, i32),
    #[serde(skip)]
    pub doc_grid: Option<TileGrid>,
    #[serde(skip)]
    pub is_generating_mips: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for TabData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabData")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("doc_width", &self.doc_width)
            .field("doc_height", &self.doc_height)
            .finish()
    }
}

impl TabData {
    pub fn new(name: String, tile_size: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            has_image: false,
            color_conversion: None,
            tile_size,
            layers: Vec::new(),
            doc_width: 0,
            doc_height: 0,
            doc_origin: (0, 0),
            doc_grid: None,
            is_generating_mips: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn base_dir(&self) -> PathBuf {
        std::env::temp_dir()
            .join("pixors")
            .join(self.id.to_string())
    }

    /// Compute document bounding box from visible layers, rebuilding doc_grid.
    pub fn recompute_doc_bounds(&mut self) {
        let (w, h, ox, oy) = if self.layers.is_empty() {
            (0, 0, 0, 0)
        } else {
            let mut min_x = i32::MAX;
            let mut min_y = i32::MAX;
            let mut max_x = i32::MIN;
            let mut max_y = i32::MIN;
            for l in &self.layers {
                // M9: include ALL layers in bbox, not just visible ones.
                // Hidden layers contribute zero pixels but the canvas shouldn't shrink.
                min_x = min_x.min(l.offset.0);
                min_y = min_y.min(l.offset.1);
                max_x = max_x.max(l.offset.0 + l.width as i32);
                max_y = max_y.max(l.offset.1 + l.height as i32);
            }
            if min_x == i32::MAX {
                (0, 0, 0, 0)
            } else {
                (
                    (max_x - min_x).max(0) as u32,
                    (max_y - min_y).max(0) as u32,
                    min_x,
                    min_y,
                )
            }
        };
        self.doc_width = w;
        self.doc_height = h;
        self.doc_origin = (ox, oy);
        self.doc_grid = if w > 0 && h > 0 {
            Some(TileGrid::new(w, h, self.tile_size))
        } else {
            None
        };
    }

    /// Composition signature — hashed from visible layer state.
    /// Changes when opacity/visible/offset/blend change, causing cache
    /// to naturally invalidate old composite tiles.
    pub fn composition_sig(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        for l in &self.layers {
            if !l.visible {
                continue;
            }
            l.id.hash(&mut h);
            l.offset.hash(&mut h);
            l.opacity.to_bits().hash(&mut h);
            (l.blend_mode as u8).hash(&mut h);
        }
        h.finish()
    }

    /// Build LayerView references for the given MIP level.
    pub fn is_mip_ready(&self, mip_level: usize) -> bool {
        if mip_level == 0 {
            return true;
        }
        self.layers.iter().all(|l| {
            l.mip_pyramid
                .level(mip_level)
                .map(|lv| lv.generated)
                .unwrap_or(false)
        })
    }

    /// Check if display MIPs (RAM) are generated at the given level for all layers.
    /// Check if display MIPs (RAM) have any tile at the given level.
    pub fn is_display_mip_ready(&self, mip_level: usize) -> bool {
        self.layers.iter().any(|l| {
            l.viewport
                .get(
                    mip_level as u32,
                    crate::image::TileCoord::new(
                        mip_level as u32,
                        0,
                        0,
                        self.tile_size,
                        self.doc_width,
                        self.doc_height,
                    ),
                )
                .is_some()
        })
    }

    /// Build LayerView references for the given MIP level.
    pub fn layer_views_for_mip(&self, mip_level: usize) -> Vec<LayerView<'_>> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .map(|l| {
                let actual_mip = mip_level as u32;
                let (w, h) = (
                    (l.width >> mip_level).max(1),
                    (l.height >> mip_level).max(1),
                );
                let comp_offset = (
                    (l.offset.0 - self.doc_origin.0) >> actual_mip,
                    (l.offset.1 - self.doc_origin.1) >> actual_mip,
                );
                LayerView {
                    id: l.id,
                    store: &l.tile_store,
                    size: (w, h),
                    offset: comp_offset,
                    opacity: l.opacity,
                    blend: l.blend_mode,
                    mip_level: actual_mip,
                }
            })
            .collect()
    }

    /// Open an image using the new pipeline (Job/Source/Sink).
    pub async fn open_image_v2(
        &mut self,
        path: impl AsRef<Path>,
        tab_id: Uuid,
        frame_tx: &tokio::sync::mpsc::UnboundedSender<crate::server::ws::types::ClientFrame>,
        vp_cb: Option<Arc<dyn Fn(u32, crate::image::TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
    ) -> Result<(), Error> {
        self.close_image().await;
        let path = path.as_ref();
        self.color_conversion = Some(ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB)?);
        let base = self.base_dir();
        self.has_image = true;

        let reader = crate::io::all_readers()
            .iter()
            .find(|r| r.can_handle(path))
            .copied()
            .ok_or_else(|| Error::unsupported_sample_type("No reader for file"))?;
        let info = reader.read_document_info(path)?;

        for layer_idx in 0..info.layer_count {
            let meta = reader.read_layer_metadata(path, layer_idx)?;
            let w = meta.desc.width;
            let h = meta.desc.height;
            let src_cs = meta.desc.color_space;

            let mip_base = base.join(format!("layer_{}_mips", self.layers.len()));
            std::fs::create_dir_all(&mip_base)?;
            let mip = MipPyramid::new(w, h, self.tile_size, mip_base.clone())?;

            let store_path = base.join(format!("layer_{}", self.layers.len()));
            let store = Arc::new(WorkingWriter::new(store_path, self.tile_size, w, h)?);

            let mut viewport = Viewport::new();
            viewport.on_tile_added = vp_cb.clone();
            let viewport = Arc::new(viewport);

            // ── New pipeline ────────────────────────────────────────────

            use crate::pipeline::job::Job;
            use crate::pipeline::source::file::FileImageSource;
            use crate::pipeline::operation::color::ColorConvertOperation;
            use crate::pipeline::operation::mip::MipOp;

            let max_mip = (w.max(h) as f32).log2().ceil() as u32;

            let mut branches = Job::from_source(FileImageSource::new(path, self.tile_size))
                .then(ColorConvertOperation::with_conv(
                    src_cs.converter_to(ColorSpace::ACES_CG)?,
                    crate::pixel::AlphaPolicy::PremultiplyOnPack,
                ))
                .then(MipOp::new(self.tile_size, max_mip, w, h))
                .split(2);

            let br1 = branches.remove(0);
            let br2 = branches.remove(0);

            let wk_job = br1.sink(WorkingSink::new(
                Arc::clone(&store),
                ColorSpace::ACES_CG.converter_to(ColorSpace::ACES_CG).unwrap(),
            ));
            let vp_job = br2.sink(ViewportSink::new(
                Arc::clone(&viewport),
                ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap(),
            ));

            // Run both sinks in background — tiles stream live via callback
            let wk_handle = std::thread::spawn(move || {
                wk_job.join();
            });
            let vp_handle = {
                let vp = Arc::clone(&viewport);
                std::thread::spawn(move || {
                    vp_job.join();
                    tracing::info!("[Pipeline] ViewportSink: {} tiles cached", vp.tile_count());
                })
            };

            self.layers.push(LayerSlot {
                id: Uuid::new_v4(),
                tile_store: store,
                mip_pyramid: mip,
                mip_base_dir: mip_base,
                width: w,
                height: h,
                offset: meta.offset,
                opacity: 1.0,
                visible: true,
                blend_mode: crate::image::BlendMode::Normal,
                viewport,
                disk_handle: Some(wk_handle),
            });
        }

        self.recompute_doc_bounds();
        Ok(())
    }

    pub async fn close_image(&mut self) {
        self.color_conversion = None;
        self.has_image = false;
        self.layers.clear();
        self.doc_width = 0;
        self.doc_height = 0;
        self.doc_grid = None;
    }

    pub fn image_info(&self) -> Option<(u32, u32)> {
        if self.has_image {
            Some((self.doc_width, self.doc_height))
        } else {
            None
        }
    }

    pub fn tile_grid(&self) -> Option<&TileGrid> {
        self.doc_grid.as_ref()
    }

    pub fn tile_grid_for_mip(&self, mip_level: usize) -> Option<TileGrid> {
        if mip_level == 0 {
            self.doc_grid.clone()
        } else {
            let lvl = mip_level as u32;
            let w = (self.doc_width >> lvl).max(1);
            let h = (self.doc_height >> lvl).max(1);
            Some(TileGrid::new(w, h, self.tile_size))
        }
    }

    pub async fn get_tile_rgba8(
        &self,
        tile: TileCoord,
        mip_level: usize,
    ) -> Result<Arc<Vec<u8>>, Error> {
        let mip = mip_level as u32;
        let visible_count = self.layers.iter().filter(|l| l.visible).count();

        // Single layer: per-layer display cache IS the final output
        if visible_count == 1
            && let Some(layer) = self.layers.iter().find(|l| l.visible)
            && let Some(rgba8) = layer.viewport.get(mip, tile)
        {
            return Ok(rgba8);
        }

        // Multi-layer or cache miss: always composite
        let views = self.layer_views_for_mip(mip_level);
        let composed: Vec<Rgba<f16>> = composite::composite_tile(&CompositeRequest {
            layers: &views,
            coord: tile,
            tile_size: self.tile_size,
        })?;

        let conv = self
            .color_conversion
            .as_ref()
            .ok_or_else(|| Error::invalid_param("Color conversion not initialized"))?;
        let rgba8 = crate::image::Tile::new(tile, composed)
            .to_srgb_u8(conv)
            .data;
        Ok(rgba8)
    }
}

impl Drop for TabData {
    fn drop(&mut self) {
        self.layers.clear();
    }
}

/// Manages tile caching and cross-tab orchestration.
/// Tab state itself lives in `TabSessionData` (owned by each session).
#[derive(Debug)]
pub struct TabService {
    default_tile_size: u32,
}

#[async_trait]
impl Service for TabService {
    type Command = TabCommand;
    type Event = TabEvent;

    async fn handle_command(
        &self,
        cmd: TabCommand,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    ) {
        self.handle_command_impl(cmd, state, ctx).await
    }
}

impl TabService {
    pub fn new(default_tile_size: u32) -> Self {
        Self { default_tile_size }
    }

    /// Creates a new `TabData` (does NOT store it — caller adds to session).
    pub fn create_tab_data(&self, name: String) -> TabData {
        TabData::new(name, self.default_tile_size)
    }

    /// Cleans up tab resources (cache eviction). Call *after* removing tab from session.
    pub fn delete_tab_cleanup(&self, _tab_id: &Uuid) {
        // DisplayWriters live in LayerSlots and drop with the tab.
        // No global cache to evict.
    }

    /// Ensures the MIP level for the given zoom is generated, per-layer.
    pub async fn ensure_mip_level(tab: &mut TabData, zoom: f32) -> Result<(), Error> {
        let level_idx = MipPyramid::level_for_zoom(zoom);
        if level_idx == 0 {
            return Ok(());
        }

        // Ensure all background disk writes are complete before reading tiles
        for layer in &mut tab.layers {
            if let Some(h) = layer.disk_handle.take() {
                let _ = h.join();
            }
        }

        let is_gen = tab.is_generating_mips.clone();

        for layer in &mut tab.layers {
            if layer
                .mip_pyramid
                .level(level_idx)
                .map(|l| l.generated)
                .unwrap_or(false)
            {
                continue;
            }

            if is_gen.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return Ok(());
            }

            let ts = layer.tile_store.tile_size();
            let iw = layer.tile_store.image_width();
            let ih = layer.tile_store.image_height();
            let mip0_path = layer.tile_store.base_dir();
            let mip_base = layer.mip_base_dir.clone();

            let mip0_view = WorkingWriter::open(mip0_path, ts, iw, ih)?;
            let regenerated_res = tokio::task::spawn_blocking(move || {
                crate::image::MipPyramid::generate_from_mip0(&mip0_view, &mip_base)
            })
            .await
            .map_err(|e| Error::Io(std::io::Error::other(e)));

            let regenerated = match regenerated_res {
                Ok(Ok(p)) => p,
                Ok(Err(e)) => {
                    is_gen.store(false, std::sync::atomic::Ordering::SeqCst);
                    return Err(e);
                }
                Err(e) => {
                    is_gen.store(false, std::sync::atomic::Ordering::SeqCst);
                    return Err(e);
                }
            };

            layer.mip_pyramid.replace_levels(regenerated.into_levels());
            is_gen.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    /// Dispatches each command variant to its dedicated handler method.
    async fn handle_command_impl(
        &self,
        cmd: TabCommand,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        match cmd {
            TabCommand::CreateTab => self.handle_create_tab(state, ctx).await,
            TabCommand::CloseTab { tab_id } => self.handle_close_tab(tab_id, state, ctx).await,
            TabCommand::ActivateTab { tab_id } => {
                self.handle_activate_tab(tab_id, state, ctx).await
            }
            TabCommand::GetTabState => self.handle_get_tab_state(state, ctx).await,
            TabCommand::MarkTilesDirty { tab_id, regions } => {
                self.handle_mark_tiles_dirty(tab_id, &regions, state, ctx)
                    .await
            }
        }
    }

    // ── Command handlers ──────────────────────────────────────────────

    async fn handle_get_tab_state(
        &self,
        state: &Arc<crate::server::app::AppState>,
        ctx: &crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        state
            .session_manager
            .with_tab_session(&ctx.session_id, |ts| {
                let tabs: Vec<TabInfo> = ts
                    .tabs
                    .values()
                    .map(|t| TabInfo {
                        id: t.id,
                        name: t.name.clone(),
                        created_at: t.created_at,
                        has_image: t.has_image,
                        width: t.doc_width,
                        height: t.doc_height,
                    })
                    .collect();
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::Tab(TabEvent::TabState {
                        tabs,
                        active_tab_id: ts.active_tab_id,
                    }),
                );
            })
            .await;
    }

    async fn handle_create_tab(
        &self,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        let name = "New Tab".to_string();
        let tab = self.create_tab_data(name.clone());
        let tab_id = tab.id;
        state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab))
            .await;
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tab(TabEvent::TabCreated { tab_id, name }),
        );
    }

    async fn handle_close_tab(
        &self,
        tab_id: Uuid,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        let removed = state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.remove(&tab_id).is_some())
            .await
            .unwrap_or(false);
        self.delete_tab_cleanup(&tab_id);
        if removed {
            send_session_event(
                &ctx.frame_tx,
                &EngineEvent::Tab(TabEvent::TabClosed { tab_id }),
            );
        }
    }

    async fn handle_activate_tab(
        &self,
        tab_id: Uuid,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        tracing::debug!("activate_tab: tab={}", tab_id);
        state
            .session_manager
            .with_tab_session_mut(&ctx.session_id, |ts| ts.set_active(Some(tab_id)))
            .await;
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tab(TabEvent::TabActivated { tab_id }),
        );

        let frame_tx = ctx.frame_tx.clone();
        let session_id = ctx.session_id;
        let state = state.clone();
        let vp_state = state.viewport_service.get_viewport(&tab_id).await;
        let (gen_counter, my_gen) = state.viewport_service.next_request_gen(&tab_id).await;
        tracing::debug!("activate_tab: spawning stream for tab {}", tab_id);
        tokio::spawn(async move {
            crate::server::service::viewport::stream_tiles_for_tab(
                tab_id,
                session_id,
                frame_tx,
                state,
                vp_state,
                gen_counter,
                my_gen,
            )
            .await;
        });
    }

    async fn handle_mark_tiles_dirty(
        &self,
        tab_id: Uuid,
        regions: &[Rect],
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        let tile_grid = state
            .session_manager
            .with_tab_session(&ctx.session_id, |ts| {
                ts.get(&tab_id).and_then(|t| t.tile_grid().cloned())
            })
            .await
            .flatten();

        let mut affected_coords = Vec::new();
        if let Some(grid) = tile_grid {
            for region in regions {
                let affected = grid.tiles_in_viewport(
                    0,
                    region.x as f32,
                    region.y as f32,
                    region.width as f32,
                    region.height as f32,
                );
                affected_coords.extend(affected.iter().cloned());
            }
        }
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tab(TabEvent::TilesDirty {
                tab_id,
                regions: regions.to_vec(),
            }),
        );
        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Viewport(
                crate::server::service::viewport::ViewportEvent::TileInvalidated {
                    tab_id,
                    coords: affected_coords,
                },
            ),
        );
    }
}
