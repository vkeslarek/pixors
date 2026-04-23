//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::error::Error;
use crate::storage::{ImageSource, PngSource, TileStore, TileCache};
use crate::image::{MipPyramid, Tile, TileGrid};
use crate::pixel::Rgba;
use half::f16;
use uuid::Uuid;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// State for a single image editing tab.
pub struct TabState {
    /// Unique identifier for the tab.
    pub id: Uuid,
    /// When the tab was created (Unix timestamp).
    pub created_at: u64,
    /// Image source (decoder). None if no image loaded.
    pub source: Option<Box<dyn ImageSource>>,
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
        // Drop source (closes file handle)
        self.source = None;
        // TileStore's Drop implementation will delete temp files
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

    /// Returns immutable reference to MIP pyramid.
    pub fn mip_pyramid(&self) -> Option<&MipPyramid> {
        self.mip_pyramid.as_ref()
    }

    /// Returns mutable reference to MIP pyramid.
    pub fn mip_pyramid_mut(&mut self) -> Option<&mut MipPyramid> {
        self.mip_pyramid.as_mut()
    }

    /// Checks if the tab has an image loaded.
    pub fn has_image(&self) -> bool {
        self.source.is_some()
    }

    /// Retrieves tile pixel data (ACEScg premul f16) from cache or source.
    pub async fn get_tile_data(
        &self,
        tile_cache: &TileCache,
        tile: &Tile,
        mip_level: usize,
    ) -> Result<Arc<Vec<Rgba<f16>>>, Error> {
        if mip_level == 0 {
            let Some(source) = &self.source else {
                return Err(Error::invalid_param("No image loaded in tab"));
            };
            let Some(tile_store) = &self.tile_store else {
                return Err(Error::invalid_param("Tile store not initialized"));
            };
            tile_cache.get_or_load(self.id, tile, tile_store, source.as_ref()).await
        } else {
            let Some(mip_pyramid) = &self.mip_pyramid else {
                return Err(Error::invalid_param("MIP pyramid not initialized"));
            };
            let Some(level) = mip_pyramid.level(mip_level) else {
                return Err(Error::invalid_param(format!("MIP level {} not found", mip_level)));
            };
            if let Some(data) = level.tile_store.get(tile).await? {
                Ok(Arc::new(data))
            } else {
                Err(Error::invalid_param(format!("MIP tile not generated yet: {:?}", tile)))
            }
        }
    }

    /// Converts tile pixel data to sRGB u8.
    pub async fn get_tile_rgba8(
        &self,
        tile_cache: &TileCache,
        tile: &Tile,
        mip_level: usize,
    ) -> Result<Vec<u8>, Error> {
        let tile_grid = if mip_level == 0 {
            self.tile_grid.as_ref()
                .ok_or_else(|| Error::invalid_param("No tile grid available"))?
        } else {
            self.mip_pyramid.as_ref()
                .and_then(|p| p.level(mip_level))
                .map(|l| &l.tile_grid)
                .ok_or_else(|| Error::invalid_param("No MIP tile grid available"))?
        };
        let data = self.get_tile_data(tile_cache, tile, mip_level).await?;
        tile_grid.tile_data_to_rgba8(tile, &data)
    }
    /// Ensures the MIP level for a given zoom factor is generated and cached.
    pub async fn ensure_mip_level(&mut self, zoom: f32, tile_cache: &TileCache) -> Result<(), Error> {
        let source = self.source.as_ref().ok_or_else(|| Error::invalid_param("No source"))?;
        let tile_store = self.tile_store.as_ref().ok_or_else(|| Error::invalid_param("No base store"))?;
        let tile_grid = self.tile_grid.as_ref().ok_or_else(|| Error::invalid_param("No base grid"))?;
        let mip_pyramid = self.mip_pyramid.as_mut().ok_or_else(|| Error::invalid_param("No MIP pyramid"))?;
        
        mip_pyramid.ensure_level_for_zoom(zoom, tile_cache, tile_store, tile_grid, source.as_ref()).await.map(|_| ())
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
}

impl TabService {
    /// Creates a new tab manager with the given default tile size.
    pub fn new(default_tile_size: u32) -> Self {
        Self {
            tabs: RwLock::new(std::collections::HashMap::new()),
            tile_cache: Arc::new(TileCache::new(256)), // default cache capacity: 256 tiles
            default_tile_size,
        }
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
        tile: &Tile,
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

    /// Deletes a tab, freeing all its resources.
    pub async fn delete_tab(&self, tab_id: &Uuid) -> bool {
        // Evict tab's tiles from cache before removing tab state
        self.tile_cache.evict_tab(tab_id).await;
        let mut tabs = self.tabs.write().await;
        tabs.remove(tab_id).is_some()
    }

    /// Lists all active tab IDs.
    pub async fn list_tabs(&self) -> Vec<Uuid> {
        let tabs = self.tabs.read().await;
        tabs.keys().cloned().collect()
    }

}
