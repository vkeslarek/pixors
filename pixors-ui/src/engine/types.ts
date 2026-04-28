// Engine event and command types (mirror Rust enums)

export interface TabData {
  id: string;
  name: string;
  created_at: number;
  has_image: boolean;
  width: number;
  height: number;
}

export type PixelFormat = 'rgba8' | 'rgba16' | 'rgba32f' | 'rgba16f';

export interface TileRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LayerInfo {
  id: string;
  name: string;
  visible: boolean;
  opacity: number;
  blend_mode: string;
  width: number;
  height: number;
  offset_x: number;
  offset_y: number;
}

// -----------------------------------------------------------------------------
// EngineEvent (broadcast from engine to all clients)
// -----------------------------------------------------------------------------

export type EngineEvent =
  | { type: 'tab_state'; tabs: TabData[]; active_tab_id: string | null }
  | { type: 'heartbeat' }
  | { type: 'tab_created'; tab_id: string; name: string }
  | { type: 'tab_closed'; tab_id: string }
  | { type: 'tab_activated'; tab_id: string }
  | { type: 'image_loaded'; tab_id: string; width: number; height: number; format: PixelFormat; layer_count: number }
  | { type: 'image_closed'; tab_id: string }
  | { type: 'image_load_progress'; tab_id: string; percent: number }
  | { type: 'tiles_complete' }
  | { type: 'tiles_dirty'; tab_id: string; regions: TileRect[] }
  | { type: 'layer_state'; tab_id: string; layers: LayerInfo[] }
  | { type: 'layer_changed'; tab_id: string; layer_id: string; field: string; composition_sig: number }
  | { type: 'doc_size_changed'; tab_id: string; width: number; height: number }
  | { type: 'mip_level_ready'; tab_id: string; level: number; width: number; height: number }
  | { type: 'viewport_state'; tab_id: string; zoom: number; pan_x: number; pan_y: number }
  | { type: 'viewport_updated'; tab_id: string; zoom: number; pan_x: number; pan_y: number }
  | { type: 'tool_state'; tool: string }
  | { type: 'tool_changed'; tool: string }
  | { type: 'error'; message: string };

// -----------------------------------------------------------------------------
// EngineCommand (sent from client to engine)
// -----------------------------------------------------------------------------

export type EngineCommand =
  | { type: 'create_tab' }
  | { type: 'close_tab'; tab_id: string }
  | { type: 'activate_tab'; tab_id: string }
  | { type: 'get_tab_state' }
  | { type: 'open_file'; tab_id: string; path: string }
  | { type: 'open_file_dialog'; tab_id?: string }
  | { type: 'get_layer_state'; tab_id: string }
  | { type: 'set_layer_visible'; tab_id: string; layer_id: string; visible: boolean }
  | { type: 'set_layer_opacity'; tab_id: string; layer_id: string; opacity: number }
  | { type: 'set_layer_offset'; tab_id: string; layer_id: string; x: number; y: number }
  | { type: 'get_viewport_state'; tab_id: string }
  | { type: 'viewport_update'; x: number; y: number; w: number; h: number; zoom: number }
  | { type: 'request_tiles'; tab_id: string; x: number; y: number; w: number; h: number; zoom: number }
  | { type: 'get_tool_state' }
  | { type: 'select_tool'; tool: string }
  | { type: 'heartbeat' }
  | { type: 'screenshot' }
  | { type: 'close' };
