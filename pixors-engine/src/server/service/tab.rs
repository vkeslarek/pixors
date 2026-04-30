//! Tab management for isolated image editing contexts with lazy tile storage.

use crate::composite::LayerView;
use crate::error::Error;
use crate::image::{TileCoord, TileGrid};
use async_trait::async_trait;
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
pub struct Tabs {
    pub tabs: HashMap<Uuid, Tab>,
    pub active_tab_id: Option<Uuid>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            active_tab_id: None,
        }
    }

    pub fn add(&mut self, tab: Tab) {
        self.tabs.insert(tab.id, tab);
    }

    pub fn remove(&mut self, tab_id: &Uuid) -> Option<Tab> {
        if self.active_tab_id == Some(*tab_id) {
            self.active_tab_id = None;
        }
        self.tabs.remove(tab_id)
    }

    pub fn get(&self, tab_id: &Uuid) -> Option<&Tab> {
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
        tabs: Vec<TabState>,
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
pub struct TabState {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    pub has_image: bool,
    pub width: u32,
    pub height: u32,
}

use crate::server::service::working_image::{LayerSlot, WorkingImage};

/// State for a single image editing tab.
#[derive(Serialize)]
pub struct Tab {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    #[serde(skip)]
    pub image: Option<WorkingImage>,
    pub tile_size: u32,
}

impl std::fmt::Debug for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabData")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("has_image", &self.image.is_some())
            .finish()
    }
}

impl Tab {
    pub fn new(name: String, tile_size: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            image: None,
            tile_size,
        }
    }

    pub fn has_image(&self) -> bool {
        self.image.is_some()
    }
    pub fn doc_width(&self) -> u32 {
        self.image.as_ref().map(|w| w.doc_width).unwrap_or(0)
    }
    pub fn doc_height(&self) -> u32 {
        self.image.as_ref().map(|w| w.doc_height).unwrap_or(0)
    }
    pub fn doc_origin(&self) -> (i32, i32) {
        self.image.as_ref().map(|w| w.doc_origin).unwrap_or((0, 0))
    }
    pub fn layers(&self) -> &[LayerSlot] {
        self.image
            .as_ref()
            .map(|w| w.layers.as_slice())
            .unwrap_or(&[])
    }
    pub fn layers_mut(&mut self) -> &mut Vec<LayerSlot> {
        &mut self.image.as_mut().expect("no image loaded").layers
    }
    pub fn base_dir(&self) -> PathBuf {
        std::env::temp_dir()
            .join("pixors")
            .join(self.id.to_string())
    }

    // ── Delegates to WorkingImage ──────────────────────────────────────
    pub fn recompute_doc_bounds(&mut self) {
        if let Some(ref mut w) = self.image {
            w.recompute_doc_bounds();
        }
    }
    pub fn composition_sig(&self) -> u64 {
        self.image
            .as_ref()
            .map(|w| w.composition_sig())
            .unwrap_or(0)
    }
    pub fn is_mip_ready(&self, mip_level: usize) -> bool {
        self.image
            .as_ref()
            .map(|w| w.is_mip_ready(mip_level))
            .unwrap_or(false)
    }
    pub fn is_display_mip_ready(&self, mip_level: usize) -> bool {
        self.image
            .as_ref()
            .map(|w| w.is_display_mip_ready(mip_level))
            .unwrap_or(false)
    }
    pub fn layer_views_for_mip(&self, mip_level: usize) -> Vec<LayerView<'_>> {
        self.image
            .as_ref()
            .map(|w| w.layer_views_for_mip(mip_level))
            .unwrap_or_default()
    }
    pub fn image_info(&self) -> Option<(u32, u32)> {
        self.image.as_ref().and_then(|w| w.image_info())
    }
    pub fn tile_grid(&self) -> Option<&TileGrid> {
        self.image.as_ref().and_then(|w| w.tile_grid())
    }
    pub fn tile_grid_for_mip(&self, mip_level: usize) -> Option<TileGrid> {
        self.image
            .as_ref()
            .and_then(|w| w.tile_grid_for_mip(mip_level))
    }
    pub async fn close_image(&mut self) {
        if let Some(ref mut w) = self.image {
            w.close_image();
        }
    }

    pub async fn open_image_v2(
        &mut self,
        path: impl AsRef<Path>,
        vp_cb: Option<Arc<dyn Fn(u32, TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
    ) -> Result<(), Error> {
        let mut w = WorkingImage::new(self.tile_size, self.base_dir());
        w.open_image_v2(path, vp_cb).await?;
        self.image = Some(w);
        Ok(())
    }

    pub async fn get_tile_rgba8(
        &self,
        tile: TileCoord,
        mip_level: usize,
    ) -> Result<Arc<Vec<u8>>, Error> {
        match &self.image {
            Some(w) => w.get_tile_rgba8(tile, mip_level).await,
            None => Err(Error::invalid_param("No image loaded")),
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.close_image();
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
    pub fn create_tab_data(&self, name: String) -> Tab {
        Tab::new(name, self.default_tile_size)
    }

    /// Cleans up tab resources (cache eviction). Call *after* removing tab from session.
    pub fn delete_tab_cleanup(&self, _tab_id: &Uuid) {
        // DisplayWriters live in LayerSlots and drop with the tab.
        // No global cache to evict.
    }

    /// Ensures the MIP level for the given zoom is generated, per-layer.
    pub async fn ensure_mip_level(tab: &mut Tab, zoom: f32) -> Result<(), Error> {
        if let Some(ref mut w) = tab.image {
            w.ensure_mip_level(zoom).await
        } else {
            Ok(())
        }
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
                let tabs: Vec<TabState> = ts
                    .tabs
                    .values()
                    .map(|t| {
                        let (w, h) = t.image_info().unwrap_or((0, 0));
                        TabState {
                            id: t.id,
                            name: t.name.clone(),
                            created_at: t.created_at,
                            has_image: t.has_image(),
                            width: w,
                            height: h,
                        }
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
