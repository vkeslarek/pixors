//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::color::{ColorConversion, ColorSpace};
use crate::error::Error;
use crate::image::{MipPyramid, TileCoord, TileGrid, TileRect};
use crate::pixel::PixelFormat;
use crate::storage::{ImageSource, PngSource, TileCache, TileStore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    TilesDirty {
        tab_id: Uuid,
        regions: Vec<TileRect>,
    },
}

/// State for a single image editing tab.
pub struct TabState {
    /// Unique identifier for the tab.
    pub id: Uuid,
    /// When the tab was created (Unix timestamp).
    pub created_at: u64,
    /// Image source (decoder). None if no image loaded.
    pub source: Option<Box<dyn ImageSource>>,
    /// Pre-computed color conversion ACEScg → sRGB for display.
    pub color_conversion: Option<ColorConversion>,
    /// Disk-backed tile storage for this tab.
    pub tile_store: Option<TileStore>,
    /// Tile grid metadata (dimensions, tile layout).
    pub tile_grid: Option<TileGrid>,
    /// Tile size used for tiling (default: 256).
    pub tile_size: u32,
    /// Per-tab mip pyramid metadata.
    pub mip_pyramid: Option<MipPyramid>,
    /// Image dimensions cached from source.
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for TabState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabState")
            .field("id", &self.id)
            .field("created_at", &self.created_at)
            .field("source", &self.source.as_ref().map(|_| "ImageSource"))
            .field("tile_store", &self.tile_store.as_ref().map(|_| "TileStore"))
            .field("tile_grid", &self.tile_grid)
            .field("tile_size", &self.tile_size)
            .field("mip_pyramid", &self.mip_pyramid.as_ref().map(|p| p.levels().len()))
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl TabState {
    /// Creates a new empty tab.
    pub fn new(tile_size: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
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
        }
    }

    /// Opens an image file in this tab.
    pub async fn open_image(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        // Close any existing image first
        self.close_image().await;

        let source = PngSource::open(path).await?;
        let (width, height) = source.dimensions();
        
        // Create tile store for this tab
        let tile_store = TileStore::new(&self.id, self.tile_size, width, height)?;
        
        // Create tile grid metadata
        let tile_grid = TileGrid::new(width, height, self.tile_size);
        let mip_pyramid = MipPyramid::new(width, height, self.tile_size, &self.id)?;
        
        self.color_conversion = Some(ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB)?);
        self.source = Some(Box::new(source));
        self.tile_store = Some(tile_store);
        self.tile_grid = Some(tile_grid);
        self.mip_pyramid = Some(mip_pyramid);
        self.width = width;
        self.height = height;
        
        Ok(())
    }

    /// Closes the currently loaded image, freeing all associated resources.
    pub async fn close_image(&mut self) {
        self.color_conversion = None;
        self.source = None;
        self.tile_store = None;
        self.tile_grid = None;
        self.mip_pyramid = None;
        self.width = 0;
        self.height = 0;
    }

    /// Returns image dimensions if an image is loaded.
    pub fn image_info(&self) -> Option<(u32, u32)> {
        if self.source.is_some() {
            Some((self.width, self.height))
        } else {
            None
        }
    }

    /// Returns the tile grid if an image is loaded.
    pub fn tile_grid(&self) -> Option<&TileGrid> {
        self.tile_grid.as_ref()
    }

    /// Converts tile pixel data to sRGB u8 via TileCache.
    pub async fn get_tile_rgba8(
        &self,
        tile_cache: &TileCache,
        tile: TileCoord,
        mip_level: usize,
    ) -> Result<Vec<u8>, Error> {
        let store = if mip_level == 0 {
            self.tile_store.as_ref().ok_or_else(|| Error::invalid_param("Tile store not initialized"))?
        } else {
            let Some(mip_pyramid) = &self.mip_pyramid else {
                return Err(Error::invalid_param("MIP pyramid not initialized"));
            };
            let Some(level) = mip_pyramid.level(mip_level) else {
                return Err(Error::invalid_param(format!("MIP level {} not found", mip_level)));
            };
            &level.tile_store
        };
        let conv = self.color_conversion.as_ref()
            .ok_or_else(|| Error::invalid_param("Color conversion not initialized"))?;
        tile_cache.get_display(self.id, tile, store, conv).await.map(|d| (*d).clone())
    }

    /// Ensures the MIP level for the given zoom is generated (blocks CPU work).
    pub async fn ensure_mip_level(&mut self, zoom: f32) -> Result<(), Error> {
        let Some(mip_pyramid) = self.mip_pyramid.as_mut() else {
            return Err(Error::invalid_param("MIP pyramid not initialized"));
        };
        let Some(tile_store) = self.tile_store.as_ref() else {
            return Err(Error::invalid_param("Tile store not initialized"));
        };
        mip_pyramid.ensure_level_for_zoom(zoom, tile_store).await
    }
}

impl Drop for TabState {
    fn drop(&mut self) {
        // Ensure tile store is dropped (which cleans up temp files)
        self.tile_store = None;
    }
}

/// Manages multiple concurrent tabs.
#[derive(Debug)]
pub struct TabService {
    pub(crate) tabs: RwLock<std::collections::HashMap<Uuid, TabState>>,
    tile_cache: Arc<TileCache>,
    default_tile_size: u32,
    active_tab: RwLock<Option<Uuid>>,
}

impl TabService {
    /// Creates a new tab manager with the given default tile size.
    pub fn new(default_tile_size: u32) -> Self {
        Self {
            tabs: RwLock::new(std::collections::HashMap::new()),
            tile_cache: Arc::new(TileCache::new()),
            default_tile_size,
            active_tab: RwLock::new(None),
        }
    }

    /// Returns the currently active tab ID, if any.
    pub async fn active_tab(&self) -> Option<Uuid> {
        *self.active_tab.read().await
    }

    /// Sets the active tab ID.
    pub async fn set_active_tab(&self, tab_id: Option<Uuid>) {
        *self.active_tab.write().await = tab_id;
    }

    /// Creates a new tab and returns its ID.
    pub async fn create_tab(&self) -> Uuid {
        let tab = TabState::new(self.default_tile_size);
        let id = tab.id;
        
        let mut tabs = self.tabs.write().await;
        tabs.insert(id, tab);
        
        id
    }

    /// Opens an image in a tab.
    pub async fn open_image(&self, tab_id: &Uuid, path: impl AsRef<Path>) -> Result<(), Error> {
        let mut tabs = self.tabs.write().await;
        if let Some(tab) = tabs.get_mut(tab_id) {
            tab.open_image(path).await
        } else {
            Err(Error::invalid_param(format!("Tab {} not found", tab_id)))
        }
    }

    /// Stream tiles from source into TileStore (call after open_image).
    pub async fn stream_tiles_to_store(&self, tab_id: &Uuid) -> Result<(), Error> {
        let tabs = self.tabs.read().await;
        let tab = tabs.get(tab_id)
            .ok_or_else(|| Error::invalid_param(format!("Tab {} not found", tab_id)))?;

        let source = tab.source.as_ref()
            .ok_or_else(|| Error::invalid_param("No image loaded in tab"))?;
        let store = tab.tile_store.as_ref()
            .ok_or_else(|| Error::invalid_param("Tile store not initialized"))?;

        source.stream_to_store(self.default_tile_size, store, *tab_id).await
    }

    /// Gets image info for a tab.
    pub async fn image_info(&self, tab_id: &Uuid) -> Option<(u32, u32)> {
        let tabs = self.tabs.read().await;
        tabs.get(tab_id).and_then(|t| t.image_info())
    }

    /// Gets the tile grid for a tab.
    pub async fn tile_grid(&self, tab_id: &Uuid) -> Option<TileGrid> {
        let tabs = self.tabs.read().await;
        tabs.get(tab_id).and_then(|t| t.tile_grid().cloned())
    }

    /// Retrieves tile pixel data as sRGB u8.
    pub async fn get_tile_rgba8(
        &self,
        tab_id: &Uuid,
        tile: TileCoord,
        mip_level: usize,
    ) -> Result<Vec<u8>, Error> {
        let tabs = self.tabs.read().await;
        let tab = tabs.get(tab_id)
            .ok_or_else(|| Error::invalid_param(format!("Tab {} not found", tab_id)))?;
        tab.get_tile_rgba8(&self.tile_cache, tile, mip_level).await
    }

    /// Returns a reference to the tile cache.
    pub fn tile_cache(&self) -> &Arc<TileCache> {
        &self.tile_cache
    }

    /// Ensures the MIP level for the given zoom is generated (blocks on CPU work via spawn_blocking).
    pub async fn ensure_mip_level(&self, tab_id: &Uuid, zoom: f32) -> Result<(), Error> {
        let mut tabs = self.tabs.write().await;
        let tab = tabs.get_mut(tab_id)
            .ok_or_else(|| Error::invalid_param(format!("Tab {} not found", tab_id)))?;
        tab.ensure_mip_level(zoom).await
    }

    /// Deletes a tab, freeing all its resources.
    pub async fn delete_tab(&self, tab_id: &Uuid) -> bool {
        // Evict tab's tiles from cache before removing tab state
        self.tile_cache.evict_tab(tab_id);
        let mut tabs = self.tabs.write().await;
        tabs.remove(tab_id).is_some()
    }

    /// Lists all active tab IDs.
    pub async fn list_tabs(&self) -> Vec<Uuid> {
        let tabs = self.tabs.read().await;
        tabs.keys().cloned().collect()
    }

    /// Handles a `TabCommand`, broadcasting events and coordinating with other services via `state`.
    pub async fn handle_command(
        &self,
        cmd: TabCommand,
        state: &Arc<crate::server::app::AppState>,
        ctx: &mut crate::server::ws::types::ConnectionContext,
    ) {
        use crate::pixel::PixelFormat;
        use crate::server::event_bus::EngineEvent;
        use crate::server::service::system::SystemEvent;

        match cmd {
            TabCommand::CreateTab => {
                let tab_id = self.create_tab().await;
                state.event_bus.broadcast(
                    EngineEvent::Tab(TabEvent::TabCreated {
                        tab_id,
                        name: "New Tab".to_string(),
                    }),
                ).await;
            }
            TabCommand::CloseTab { tab_id } => {
                if self.active_tab().await == Some(tab_id) {
                    self.set_active_tab(None).await;
                }
                if self.delete_tab(&tab_id).await {
                    state.event_bus.broadcast(
                        EngineEvent::Tab(TabEvent::TabClosed { tab_id }),
                    ).await;
                }
            }
            TabCommand::ActivateTab { tab_id } => {
                self.set_active_tab(Some(tab_id)).await;
                state.event_bus.broadcast(
                    EngineEvent::Tab(TabEvent::TabActivated { tab_id }),
                ).await;

                // Spawn tile streaming — reader stays responsive.
                let frame_tx = ctx.frame_tx.clone();
                let state = state.clone();
                let vp_state = state.viewport_service.get_viewport(&tab_id).await;
                tokio::spawn(async move {
                    crate::server::service::viewport::stream_tiles_for_tab(
                        tab_id, frame_tx, state, vp_state,
                    )
                    .await;
                });
            }
            TabCommand::OpenFile { tab_id, path } => {
                match self.open_image(&tab_id, &path).await {
                    Ok(()) => {
                        // Stream tiles into store (populate all tiles at once)
                        if let Err(e) = self.stream_tiles_to_store(&tab_id).await {
                            tracing::error!("Failed to stream tiles: {}", e);
                            state.event_bus.broadcast(
                                EngineEvent::System(SystemEvent::Error {
                                    message: format!("Failed to stream image tiles: {}", e),
                                }),
                            ).await;
                            return;
                        }

                        if let Some((width, height)) = self.image_info(&tab_id).await {
                            state.event_bus.broadcast(
                                EngineEvent::Tab(TabEvent::ImageLoaded {
                                    tab_id,
                                    width,
                                    height,
                                    format: PixelFormat::Rgba8,
                                }),
                            ).await;
                        }
                    }
                    Err(e) => {
                        state.event_bus.broadcast(
                            EngineEvent::System(SystemEvent::Error {
                                message: format!("Failed to load image: {}", e),
                            }),
                        ).await;
                    }
                }
            }
            TabCommand::MarkTilesDirty { tab_id, regions } => {
                // Invalidate display cache for affected tiles
                let mut affected_coords = Vec::new();
                for region in &regions {
                    let tile_grid = state.tab_service.tile_grid(&tab_id).await;
                    if let Some(grid) = tile_grid {
                        let affected = grid.tiles_in_viewport(0, region.x as f32, region.y as f32, region.width as f32, region.height as f32);
                        for coord in &affected {
                            state.tab_service.tile_cache().invalidate_display(tab_id, *coord);
                            affected_coords.push(*coord);
                        }
                    }
                }
                state.event_bus.broadcast(
                    EngineEvent::Tab(TabEvent::TilesDirty { tab_id, regions: regions.clone() }),
                ).await;
                state.event_bus.broadcast(
                    EngineEvent::Viewport(crate::server::service::viewport::ViewportEvent::TileInvalidated {
                        tab_id,
                        coords: affected_coords,
                    }),
                ).await;
            }
        }
    }
}
