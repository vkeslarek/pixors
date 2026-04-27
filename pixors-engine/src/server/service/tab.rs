//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::color::ColorSpace;
use crate::composite::{self, CompositeRequest, LayerView};
use crate::convert::ColorConversion;
use crate::error::Error;
use crate::image::{BlendMode, MipPyramid, TileCoord, TileGrid};
use crate::pixel::{PixelFormat, Rgba};
use crate::storage::writer::WorkingWriter;
use async_trait::async_trait;
use half::f16;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

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
    CloseTab {
        tab_id: Uuid,
    },
    ActivateTab {
        tab_id: Uuid,
    },
    OpenFile {
        tab_id: Uuid,
        path: String,
    },
    OpenFileDialog {
        #[serde(default)]
        tab_id: Option<Uuid>,
    },
    MarkTilesDirty {
        tab_id: Uuid,
        regions: Vec<Rect>,
    },
    LayerSetVisible {
        tab_id: Uuid,
        layer_id: Uuid,
        visible: bool,
    },
    LayerSetOpacity {
        tab_id: Uuid,
        layer_id: Uuid,
        opacity: f32,
    },
    LayerSetOffset {
        tab_id: Uuid,
        layer_id: Uuid,
        x: i32,
        y: i32,
    },
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
    ImageLoaded {
        tab_id: Uuid,
        width: u32,
        height: u32,
        format: PixelFormat,
        layer_count: usize,
    },
    ImageClosed {
        tab_id: Uuid,
    },
    ImageLoadProgress {
        tab_id: Uuid,
        percent: u8,
    },
    TilesDirty {
        tab_id: Uuid,
        regions: Vec<Rect>,
    },
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
    pub viewport: Arc<crate::stream::Viewport>,
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
            l.viewport.get(mip_level as u32, crate::image::TileCoord::new(mip_level as u32, 0, 0, self.tile_size, self.doc_width, self.doc_height)).is_some()
        })
    }

    /// Build LayerView references for the given MIP level.
    pub fn layer_views_for_mip(&self, mip_level: usize) -> Vec<LayerView<'_>> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .map(|l| {
                let has_mip = mip_level == 0
                    || l.mip_pyramid
                        .level(mip_level)
                        .map(|lv| lv.generated)
                        .unwrap_or(false);
                let actual_mip = if has_mip { mip_level as u32 } else { 0 };
                let store = if actual_mip == 0 {
                    &*l.tile_store
                } else {
                    l.mip_pyramid
                        .level(mip_level)
                        .map(|level| &level.tile_store)
                        .unwrap_or(&*l.tile_store)
                };
                let (w, h) = if actual_mip == 0 {
                    (l.width, l.height)
                } else {
                    l.mip_pyramid
                        .level(mip_level)
                        .map(|level| (level.width, level.height))
                        .unwrap_or((l.width >> mip_level, l.height >> mip_level))
                };
                let comp_offset = (
                    (l.offset.0 - self.doc_origin.0) >> actual_mip,
                    (l.offset.1 - self.doc_origin.1) >> actual_mip,
                );
                LayerView {
                    id: l.id,
                    store,
                    size: (w.max(1), h.max(1)),
                    offset: comp_offset,
                    opacity: l.opacity,
                    blend: l.blend_mode,
                    mip_level: actual_mip,
                }
            })
            .collect()
    }

    pub async fn open_image(
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

        use crate::stream::{TileSource, ImageFileSource, ColorConvertPipe, MipPipe, Pipe, TileSink, Viewport, ViewportSink, WorkingSink};
        use crate::stream::tee;

        // Read metadata first to know source color space + dimensions
        let reader = crate::io::all_readers()
            .iter().find(|r| r.can_handle(path)).copied()
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

            // Create disk storage (shared Arc for WorkingSink + later MIP gen reads)
            let store_path = base.join(format!("layer_{}", self.layers.len()));
            let store = Arc::new(WorkingWriter::new(
                store_path, self.tile_size, w, h,
            )?);

            // Create viewport (RAM tile cache, shared with server)
            let mut viewport = Viewport::new();
            viewport.on_tile_added = vp_cb.clone();
            let viewport = Arc::new(viewport);

            // ── Stream pipeline ──────────────────────────────────────
            let rx = {
                let _sw = crate::debug_stopwatch!("OpenFile:stream_tiles");
                ImageFileSource::new(path.to_path_buf(), self.tile_size, 0).open()?
            };

            // Color convert to sRGB u8 (display), then MIP
            let mut rx = ColorConvertPipe::new(
                src_cs, ColorSpace::SRGB, crate::pixel::AlphaPolicy::Straight, false, meta.desc.clone(),
            )?.pipe(rx);
            let num_levels = {
                let mut lw = w; let mut lh = h; let mut n = 0u32;
                while lw > 1 || lh > 1 { lw = (lw/2).max(1); lh = (lh/2).max(1); n += 1; }
                n.min(6)
            };
            rx = MipPipe::new(self.tile_size, num_levels).pipe(rx);

            let mut rx_vec = tee(rx, 3);
            let pr_rx = rx_vec.pop().unwrap();
            let wk_rx = rx_vec.pop().unwrap();
            let vp_rx = rx_vec.pop().unwrap();

            // Working branch: convert sRGB u8 → f16 ACEScg premul, then write to disk
            // Data is already RGBA u8 from the first ColorConvertPipe
            let wk_rx = ColorConvertPipe::new(
                ColorSpace::SRGB, ColorSpace::ACES_CG, crate::pixel::AlphaPolicy::PremultiplyOnPack, true,
                crate::image::buffer::BufferDesc::rgba8_interleaved(1, 1, ColorSpace::SRGB, crate::image::AlphaMode::Straight),
            )?.pipe(wk_rx);

            let vp_sink = ViewportSink::new(Arc::clone(&viewport));
            let wk_sink = WorkingSink::new(Arc::clone(&store));

            // Progress branch: emit ImageLoadProgress events
            use crate::stream::ProgressSink;
            let frame_tx_progress = frame_tx.clone();
            let tab_id_progress = tab_id;
            let pr_sink = ProgressSink::new(move |percent| {
                let event = crate::server::event_bus::EngineEvent::Tab(crate::server::service::tab::TabEvent::ImageLoadProgress {
                    tab_id: tab_id_progress,
                    percent,
                });
                use crate::server::ws::types::send_session_event;
                send_session_event(&frame_tx_progress, &event);
            });

            let _vp_handle = vp_sink.run(vp_rx);
            let wk_handle = wk_sink.run(wk_rx);
            let _pr_handle = pr_sink.run(pr_rx);

            // DON'T join — tiles auto-stream to frontend via vp_cb callback
            // Disk handle joined in Drop

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
        let rgba8 = crate::image::Tile::new(tile, composed).to_srgb_u8(conv).data;
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

    /// Ensures the MIP level for the given zoom is generated synchronously (blocking).
    #[allow(dead_code)]
    pub fn ensure_mip_level_blocking(tab: &mut TabData, zoom: f32) -> Result<(), Error> {
        let level_idx = MipPyramid::level_for_zoom(zoom);
        if level_idx == 0 {
            return Ok(());
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
            let regenerated = tokio::task::block_in_place(|| {
                crate::image::MipPyramid::generate_from_mip0(&mip0_view, &mip_base)
            })?;

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
            TabCommand::OpenFile { tab_id, path } => {
                self.handle_open_file(tab_id, &path, state, ctx).await
            }
            TabCommand::OpenFileDialog { tab_id } => {
                self.handle_open_file_dialog(tab_id, state, ctx).await
            }
            TabCommand::MarkTilesDirty { tab_id, regions } => {
                self.handle_mark_tiles_dirty(tab_id, &regions, state, ctx)
                    .await
            }
            TabCommand::LayerSetVisible {
                tab_id,
                layer_id,
                visible,
            } => {
                self.handle_layer_set_visible(tab_id, layer_id, visible, state, ctx)
                    .await
            }
            TabCommand::LayerSetOpacity {
                tab_id,
                layer_id,
                opacity,
            } => {
                self.handle_layer_set_opacity(tab_id, layer_id, opacity, state, ctx)
                    .await
            }
            TabCommand::LayerSetOffset {
                tab_id,
                layer_id,
                x,
                y,
            } => {
                self.handle_layer_set_offset(tab_id, layer_id, x, y, state, ctx)
                    .await
            }
        }
    }

    // ── Command handlers ──────────────────────────────────────────────

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

    async fn handle_open_file(
        &self,
        tab_id: Uuid,
        path: &str,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::debug_stopwatch;
        use crate::pixel::PixelFormat;
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::system::SystemEvent;
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
                        &EngineEvent::System(SystemEvent::Error {
                            message: format!("Failed to load image: {}", e),
                        }),
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
            &EngineEvent::Tab(TabEvent::ImageLoaded {
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
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
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
                    let tab = self.create_tab_data(name.clone());
                    let new_id = tab.id;
                    state
                        .session_manager
                        .with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab))
                        .await;
                    send_session_event(
                        &ctx.frame_tx,
                        &EngineEvent::Tab(TabEvent::TabCreated {
                            tab_id: new_id,
                            name,
                        }),
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
            let cmd = TabCommand::OpenFile {
                tab_id: target_tab_id,
                path: path_str.to_string(),
            };
            Box::pin(self.handle_command(cmd, state, ctx)).await;
        }
    }

    // ── Layer mutation handlers ─────────────────────────────────────

    async fn handle_layer_set_visible(
        &self,
        tab_id: Uuid,
        layer_id: Uuid,
        visible: bool,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
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
                &EngineEvent::Tab(TabEvent::LayerChanged {
                    tab_id,
                    layer_id,
                    field: "visible".into(),
                    composition_sig: sig,
                }),
            );
            if changed {
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::Tab(TabEvent::DocSizeChanged {
                        tab_id,
                        width: w,
                        height: h,
                    }),
                );
            }
        }
    }

    async fn handle_layer_set_opacity(
        &self,
        tab_id: Uuid,
        layer_id: Uuid,
        opacity: f32,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
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
                &EngineEvent::Tab(TabEvent::LayerChanged {
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
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
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
                &EngineEvent::Tab(TabEvent::LayerChanged {
                    tab_id,
                    layer_id,
                    field: "offset".into(),
                    composition_sig: sig,
                }),
            );
            if changed {
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::Tab(TabEvent::DocSizeChanged {
                        tab_id,
                        width: w,
                        height: h,
                    }),
                );
            }
        }
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
