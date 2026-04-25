//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::color::{ColorConversion, ColorSpace};
use crate::error::Error;
use std::path::PathBuf;
use crate::image::{MipPyramid, TileCoord, TileGrid, TileRect};
use crate::pixel::PixelFormat;
use crate::storage::{ImageSource, FormatSource, TileCache, TileStore};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

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
        regions: Vec<TileRect>,
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
        regions: Vec<TileRect>,
    },
}

/// State for a single image editing tab.
#[derive(Serialize)]
pub struct TabData {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    #[serde(skip)]
    pub source: Option<Box<dyn ImageSource>>,
    #[serde(skip)]
    pub color_conversion: Option<ColorConversion>,
    #[serde(skip)]
    pub tile_store: Option<TileStore>,
    #[serde(skip)]
    pub tile_grid: Option<TileGrid>,
    pub tile_size: u32,
    #[serde(skip)]
    pub mip_pyramid: Option<MipPyramid>,
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub is_generating_mips: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for TabData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabData")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("width", &self.width)
            .field("height", &self.height)
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
            source: None,
            color_conversion: None,
            tile_store: None,
            tile_grid: None,
            tile_size,
            mip_pyramid: None,
            width: 0,
            height: 0,
            is_generating_mips: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Returns the base directory for tile storage.
    fn base_dir(&self) -> PathBuf {
        std::env::temp_dir()
            .join("pixors")
            .join(self.id.to_string())
    }

    pub async fn open_image(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.close_image().await;

        let source = FormatSource::open(path).await?;
        let (width, height) = source.dimensions();

        let tile_store = TileStore::new(self.base_dir(), self.tile_size, width, height)?;
        let tile_grid = TileGrid::new(width, height, self.tile_size);
        let mip_pyramid = MipPyramid::new(width, height, self.tile_size, self.base_dir())?;

        self.color_conversion = Some(ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB)?);
        self.source = Some(Box::new(source));
        self.tile_store = Some(tile_store);
        self.tile_grid = Some(tile_grid);
        self.mip_pyramid = Some(mip_pyramid);
        self.width = width;
        self.height = height;

        Ok(())
    }

    pub async fn close_image(&mut self) {
        self.color_conversion = None;
        self.source = None;
        self.tile_store = None;
        self.tile_grid = None;
        self.mip_pyramid = None;
        self.width = 0;
        self.height = 0;
    }

    pub fn image_info(&self) -> Option<(u32, u32)> {
        if self.source.is_some() {
            Some((self.width, self.height))
        } else {
            None
        }
    }

    pub fn tile_grid(&self) -> Option<&TileGrid> {
        self.tile_grid.as_ref()
    }

    pub fn tile_grid_for_mip(&self, mip_level: usize) -> Option<TileGrid> {
        if mip_level == 0 {
            self.tile_grid().cloned()
        } else {
            self.mip_pyramid
                .as_ref()
                .and_then(|p| p.level(mip_level))
                .filter(|l| l.generated)
                .map(|l| l.tile_grid.clone())
        }
    }

    pub async fn get_tile_rgba8(
        &self,
        tile_cache: &TileCache,
        tile: TileCoord,
        mip_level: usize,
    ) -> Result<Vec<u8>, Error> {
        let store = if mip_level == 0 {
            self.tile_store
                .as_ref()
                .ok_or_else(|| Error::invalid_param("Tile store not initialized"))?
        } else {
            let Some(mip_pyramid) = &self.mip_pyramid else {
                return Err(Error::invalid_param("MIP pyramid not initialized"));
            };
            let Some(level) = mip_pyramid.level(mip_level) else {
                return Err(Error::invalid_param(format!(
                    "MIP level {} not found",
                    mip_level
                )));
            };
            &level.tile_store
        };
        let conv = self
            .color_conversion
            .as_ref()
            .ok_or_else(|| Error::invalid_param("Color conversion not initialized"))?;
        tile_cache
            .get_display(self.id, tile, store, conv)
            .await
            .map(|d| (*d).clone())
    }

    /// Stream tiles from source into TileStore (call after open_image).
    pub async fn stream_tiles_to_store(&self, tile_size: u32, on_progress: Option<Box<dyn Fn(u8) + Send>>) -> Result<(), Error> {
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| Error::invalid_param("No image loaded in tab"))?;
        let store = self
            .tile_store
            .as_ref()
            .ok_or_else(|| Error::invalid_param("Tile store not initialized"))?;
        source.stream_to_store(tile_size, store, self.id, on_progress).await
    }
}

impl Drop for TabData {
    fn drop(&mut self) {
        self.tile_store = None;
    }
}

/// Manages tile caching and cross-tab orchestration.
/// Tab state itself lives in `TabSessionData` (owned by each session).
#[derive(Debug)]
pub struct TabService {
    tile_cache: Arc<TileCache>,
    default_tile_size: u32,
}

#[async_trait]
impl Service for TabService {
    type Command = TabCommand;
    type Event = TabEvent;

    async fn handle_command(&self, cmd: TabCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
        self.handle_command_impl(cmd, state, ctx).await
    }
}

impl TabService {
    pub fn new(default_tile_size: u32) -> Self {
        Self {
            tile_cache: Arc::new(TileCache::new()),
            default_tile_size,
        }
    }

    pub fn tile_cache(&self) -> &Arc<TileCache> {
        &self.tile_cache
    }

    /// Creates a new `TabData` (does NOT store it — caller adds to session).
    pub fn create_tab_data(&self, name: String) -> TabData {
        TabData::new(name, self.default_tile_size)
    }

    /// Cleans up tab resources (cache eviction). Call *after* removing tab from session.
    pub fn delete_tab_cleanup(&self, tab_id: &Uuid) {
        self.tile_cache.evict_tab(tab_id);
    }

    /// Ensures the MIP level for the given zoom is generated.
    /// `tab` is taken out of the session so the lock is not held during CPU work.
    pub async fn ensure_mip_level(tab: &mut TabData, zoom: f32) -> Result<(), Error> {
        use crate::image::mip_level_for_zoom;

        let level_idx = mip_level_for_zoom(zoom);
        if level_idx == 0 {
            return Ok(());
        }

        // --- Read phase: check if already generated, gather params ---
        let (base_dir, tile_size, img_w, img_h, is_generating) = {
            if tab
                .mip_pyramid
                .as_ref()
                .and_then(|p| p.level(level_idx))
                .map(|l| l.generated)
                .unwrap_or(false)
            {
                return Ok(());
            }
            if tab
                .is_generating_mips
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Ok(());
            }
            let store = tab
                .tile_store
                .as_ref()
                .ok_or_else(|| Error::invalid_param("Tile store not initialized"))?;
            (
                tab.base_dir(),
                store.tile_size(),
                store.image_width(),
                store.image_height(),
                tab.is_generating_mips.clone(),
            )
        };

        // --- Heavy CPU work ---
        let mip0_view = crate::storage::TileStore::open(base_dir.clone(), tile_size, img_w, img_h)?;
        let regenerated_res = tokio::task::spawn_blocking(move || {
            let _sw = crate::debug_stopwatch!("ensure_mip_level:generate");
            crate::image::generate_from_mip0(&mip0_view, &base_dir)
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)));

        let regenerated = match regenerated_res {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                is_generating.store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
            Err(e) => {
                is_generating.store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(e);
            }
        };

        // --- Write phase: install generated levels ---
        if let Some(mip_pyramid) = &mut tab.mip_pyramid {
            mip_pyramid.replace_levels(regenerated.into_levels());
        }

        is_generating.store(false, std::sync::atomic::Ordering::SeqCst);
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
            TabCommand::ActivateTab { tab_id } => self.handle_activate_tab(tab_id, state, ctx).await,
            TabCommand::OpenFile { tab_id, path } => self.handle_open_file(tab_id, &path, state, ctx).await,
            TabCommand::OpenFileDialog { tab_id } => self.handle_open_file_dialog(tab_id, state, ctx).await,
            TabCommand::MarkTilesDirty { tab_id, regions } => self.handle_mark_tiles_dirty(tab_id, &regions, state, ctx).await,
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
        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab)).await;
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

        let removed = state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.remove(&tab_id).is_some()).await.unwrap_or(false);
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
        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.set_active(Some(tab_id))).await;
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
                tab_id, session_id, frame_tx, state, vp_state, gen_counter, my_gen,
            ).await;
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

        let tab = state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.tabs.remove(&tab_id)).await.flatten();
        let Some(mut tab) = tab else {
            tracing::error!("Tab {} not found in session", tab_id);
            return;
        };

        {
            let _sw = debug_stopwatch!("OpenFile:open_image");
            match tab.open_image(path).await {
                Ok(()) => tracing::debug!("open_image: done tab={}", tab_id),
                Err(e) => {
                    tracing::error!("Failed to load image: {}", e);
                    state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab)).await;
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

        {
            let _sw = debug_stopwatch!("OpenFile:stream_tiles");
            let frame_tx_progress = ctx.frame_tx.clone();
            let tab_id_progress = tab_id;
            let on_progress = move |percent: u8| {
                send_session_event(
                    &frame_tx_progress,
                    &EngineEvent::Tab(TabEvent::ImageLoadProgress {
                        tab_id: tab_id_progress,
                        percent,
                    }),
                );
            };
            if let Err(e) = tab.stream_tiles_to_store(self.default_tile_size, Some(Box::new(on_progress))).await {
                tracing::error!("Failed to stream tiles: {}", e);
                state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab)).await;
                send_session_event(
                    &ctx.frame_tx,
                    &EngineEvent::System(SystemEvent::Error {
                        message: format!("Failed to stream image tiles: {}", e),
                    }),
                );
                return;
            }
        }

        tracing::debug!("stream_tiles: done tab={}", tab_id);
        let (width, height) = tab.image_info().unwrap_or((0, 0));
        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab)).await;

        send_session_event(
            &ctx.frame_tx,
            &EngineEvent::Tab(TabEvent::ImageLoaded {
                tab_id, width, height,
                format: PixelFormat::Rgba8,
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
                .add_filter("Image", &["png", "tiff", "tif", "jpg", "jpeg", "exr", "hdr"])
                .pick_file()
        })
        .await
        .unwrap_or(None);

        if let Some(path_buf) = path_opt {
            if let Some(path_str) = path_buf.to_str() {
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
                        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.add(tab)).await;
                        send_session_event(
                            &ctx.frame_tx,
                            &EngineEvent::Tab(TabEvent::TabCreated { tab_id: new_id, name }),
                        );
                        state.session_manager.with_tab_session_mut(&ctx.session_id, |ts| ts.set_active(Some(new_id))).await;
                        send_session_event(
                            &ctx.frame_tx,
                            &EngineEvent::Tab(TabEvent::TabActivated { tab_id: new_id }),
                        );
                        new_id
                    }
                };
                let cmd = TabCommand::OpenFile { tab_id: target_tab_id, path: path_str.to_string() };
                Box::pin(self.handle_command(cmd, state, ctx)).await;
            }
        }
    }

    async fn handle_mark_tiles_dirty(
        &self,
        tab_id: Uuid,
        regions: &[TileRect],
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::server::event_bus::EngineEvent;
        use crate::server::ws::types::send_session_event;

        let tile_grid = state.session_manager.with_tab_session(&ctx.session_id, |ts| {
            ts.get(&tab_id).and_then(|t| t.tile_grid().cloned())
        }).await.flatten();

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
                for coord in &affected {
                    self.tile_cache.invalidate_display(tab_id, *coord);
                    affected_coords.push(*coord);
                }
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
