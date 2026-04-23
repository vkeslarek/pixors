use crate::server::event_bus::{EngineCommand, EngineEvent};
use crate::server::protocol::{ClientCommand, PixelFormat};
use crate::server::state::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitStream, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::{SentTileKey, WriterMessage};

pub async fn run_reader_task(
    mut receiver: SplitStream<WebSocket>,
    state: Arc<AppState>,
    tab_id: Option<Uuid>,
    tile_tx: mpsc::UnboundedSender<WriterMessage>,
) {
    let mut sent_tiles = HashSet::<SentTileKey>::new();
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                // Parse as EngineCommand
                if let Ok(cmd) = serde_json::from_str::<EngineCommand>(&text) {
                    // Handle engine command
                    handle_engine_command(cmd, &state, tab_id, &mut sent_tiles, &tile_tx).await;
                } else if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                    // Legacy command handling (temporary)
                    let should_close =
                        handle_legacy_command(cmd, &state, tab_id, &mut sent_tiles, &tile_tx).await;
                    if should_close {
                        break;
                    }
                } else {
                    // Invalid command, ignore
                    tracing::warn!("Invalid command received: {}", text);
                }
            }
            Message::Binary(_) => {
                // Binary messages are expected for tile data, ignore for now
            }
            Message::Close(_) => {
                tracing::info!("WebSocket closed by client");
                break;
            }
            _ => {}
        }
    }
}

/// Handle an EngineCommand by updating state and broadcasting events.
async fn handle_engine_command(
    cmd: EngineCommand,
    state: &Arc<AppState>,
    current_tab_id: Option<Uuid>,
    sent_tiles: &mut HashSet<SentTileKey>,
    tile_tx: &mpsc::UnboundedSender<WriterMessage>,
) {
    tracing::info!("EngineCommand received: {:?}", cmd);
    
    match cmd {
        EngineCommand::CreateTab => {
            // Create new tab
            let tab_id = state.create_tab().await;
            state.event_bus().broadcast(EngineEvent::TabCreated {
                tab_id,
                name: "New Tab".to_string(),
            }).await;
        }
        EngineCommand::CloseTab { tab_id } => {
            // Close tab
            let deleted = state.delete_tab(&tab_id).await;
            if deleted {
                state.event_bus().broadcast(EngineEvent::TabClosed { tab_id }).await;
            }
        }
        EngineCommand::ActivateTab { tab_id } => {
            // Activate tab (UI should switch to this tab)
            state.event_bus().broadcast(EngineEvent::TabActivated { tab_id }).await;
        }
        EngineCommand::OpenFile { tab_id, path } => {
            // Load image into tab
            match state.open_image(&tab_id, &path).await {
                Ok(()) => {
                    sent_tiles.retain(|k| k.tab_id != tab_id);
                    // Get image info
                    if let Some((width, height)) = state.image_info(&tab_id).await {
                        state.event_bus().broadcast(EngineEvent::ImageLoaded {
                            tab_id,
                            width,
                            height,
                            format: PixelFormat::Rgba8,
                        }).await;
                    }
                }
                Err(e) => {
                    state.event_bus().broadcast(EngineEvent::Error {
                        message: format!("Failed to load image: {}", e),
                    }).await;
                }
            }
        }
        EngineCommand::ViewportUpdate { x, y, w, h, zoom } => {
            // Update viewport for current tab
            if let Some(tab_id) = current_tab_id {
                // Update viewport service
                let viewport_state = crate::server::service::viewport::ViewportState {
                    width: w as u32,
                    height: h as u32,
                    zoom,
                    pan_x: x,
                    pan_y: y,
                };
                state.viewport_service().update_viewport(&tab_id, viewport_state).await;
                
                // Broadcast viewport update (lightweight, no tile data)
                state.event_bus().broadcast(EngineEvent::ViewportUpdated {
                    tab_id,
                    zoom,
                    pan_x: x,
                    pan_y: y,
                }).await;
                
                if let Err(e) = stream_visible_tiles_for_tab(
                    &state,
                    &tab_id,
                    x,
                    y,
                    w,
                    h,
                    zoom,
                    sent_tiles,
                    tile_tx,
                )
                .await
                {
                    tracing::error!("Failed to stream visible tiles: {}", e);
                }
            }
        }
        EngineCommand::SelectTool { tool } => {
            state.event_bus().broadcast(EngineEvent::ToolChanged { tool }).await;
        }
        EngineCommand::GetState => {
            let tabs = state.list_tabs().await;
            for tab_id in tabs.iter() {
                state.event_bus().broadcast(EngineEvent::TabCreated {
                    tab_id: *tab_id,
                    name: "Tab".to_string(),
                }).await;

                if let Some((width, height)) = state.image_info(tab_id).await {
                    state.event_bus().broadcast(EngineEvent::ImageLoaded {
                        tab_id: *tab_id,
                        width,
                        height,
                        format: PixelFormat::Rgba8,
                    }).await;
                }
            }

            if let Some(first_tab) = tabs.first() {
                state.event_bus().broadcast(EngineEvent::TabActivated {
                    tab_id: *first_tab,
                }).await;
            }
        }
        EngineCommand::Screenshot => {
            // TODO: Capture viewport and send as base64 PNG
        }
        EngineCommand::Close => {
            // Client requested close, connection will be closed by reader task
        }
        EngineCommand::MarkTilesDirty { tab_id, regions } => {
            // Broadcast tiles dirty event
            state.event_bus().broadcast(EngineEvent::TilesDirty {
                tab_id,
                regions,
            }).await;
        }
    }
}

/// Stream only currently visible tiles for a tab.
/// Sends tile data directly to the writer task via `tile_tx` (no event bus broadcasting).
async fn stream_visible_tiles_for_tab(
    state: &Arc<AppState>,
    tab_id: &Uuid,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    zoom: f32,
    sent_tiles: &mut HashSet<SentTileKey>,
    tile_tx: &mpsc::UnboundedSender<WriterMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let zoom = zoom.max(0.0001);
    let viewport_width = w / zoom;
    let viewport_height = h / zoom;
    
    // TODO: Enable MIP-level tile streaming once the frontend supports
    // dynamic texture resizing or coordinate remapping for sub-resolution tiles.
    // For now, always use base-level (full-res) tiles — the GPU sampler handles
    // minification via linear filtering.
    let mip_level = 0; // mip_level_for_zoom(zoom);

    // Get the tile grid (read lock only, released immediately).
    let tile_grid = {
        let Some(tile_grid) = state.tile_grid(tab_id).await else {
            return Err("No image loaded in tab".into());
        };
        tile_grid
    };
    
    // Stream tiles (no lock held — get_tile_rgba8 acquires its own read lock per call).
    let visible_tiles = tile_grid.tiles_in_viewport(x, y, viewport_width, viewport_height);

    for tile in visible_tiles {
        let key = SentTileKey {
            tab_id: *tab_id,
            x: tile.x,
            y: tile.y,
        };
        if sent_tiles.contains(&key) {
            continue;
        }

        // Get tile data via tile cache (acquires read lock internally, no conflict)
        let rgba8 = state.get_tile_rgba8(tab_id, tile, mip_level).await?;
        let size = rgba8.len();
        
        let json = serde_json::json!({
            "type": "tile_data",
            "tab_id": tab_id,
            "x": tile.x,
            "y": tile.y,
            "width": tile.width,
            "height": tile.height,
            "mip_level": mip_level,
            "size": size,
        });

        // Send directly to this connection's writer task (no cloning to other subscribers)
        if tile_tx.send(WriterMessage::TileData {
            json: json.to_string(),
            data: rgba8,
        }).is_err() {
            return Err("Writer task closed".into());
        }

        sent_tiles.insert(key);
    }
    
    let _ = tile_tx.send(WriterMessage::TilesComplete);
    Ok(())
}

/// Dispatches a legacy ClientCommand to the appropriate handler.
/// Returns `true` if the connection should be closed.
async fn handle_legacy_command(
    cmd: ClientCommand,
    state: &Arc<AppState>,
    tab_id: Option<Uuid>,
    sent_tiles: &mut HashSet<SentTileKey>,
    tile_tx: &mpsc::UnboundedSender<WriterMessage>,
) -> bool {
    // For now, we still need to handle legacy commands.
    // This will be removed once frontend migrates to EngineCommand.
    match cmd {
        ClientCommand::LoadImage { path } => {
            // We'll handle via engine command
            if let Some(tab_id) = tab_id {
                handle_engine_command(
                    EngineCommand::OpenFile { tab_id, path },
                    state,
                    Some(tab_id),
                    sent_tiles,
                    tile_tx,
                )
                .await;
            }
            false
        }
        ClientCommand::GetImageInfo => {
            // Not yet implemented in new system
            false
        }
        ClientCommand::ApplyOperation { op: _, params: _ } => {
            false
        }
        ClientCommand::Close => {
            tracing::info!("Client requested close");
            true
        }
    }
}
