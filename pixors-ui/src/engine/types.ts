// Engine event and command types (mirror Rust enums)

export type PixelFormat = 'rgba8' | 'rgba16' | 'rgba32f' | 'rgba16f';

export interface TileRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// -----------------------------------------------------------------------------
// EngineEvent (broadcast from engine to all clients)
// -----------------------------------------------------------------------------

export type EngineEvent =
  | { type: 'tab_created'; tab_id: string; name: string }
  | { type: 'tab_closed'; tab_id: string }
  | { type: 'tab_activated'; tab_id: string }
  | { type: 'image_loaded'; tab_id: string; width: number; height: number; format: PixelFormat }
  | { type: 'image_closed'; tab_id: string }
  | { type: 'tiles_complete' }
  | { type: 'tiles_dirty'; tab_id: string; regions: TileRect[] }
  | { type: 'tool_changed'; tool: string }
  | { type: 'viewport_updated'; tab_id: string; zoom: number; pan_x: number; pan_y: number }
  | { type: 'error'; message: string };

// -----------------------------------------------------------------------------
// EngineCommand (sent from client to engine)
// -----------------------------------------------------------------------------

export type EngineCommand =
  | { type: 'create_tab' }
  | { type: 'close_tab'; tab_id: string }
  | { type: 'activate_tab'; tab_id: string }
  | { type: 'open_file'; tab_id: string; path: string }
  | { type: 'viewport_update'; x: number; y: number; w: number; h: number; zoom: number }
  | { type: 'request_tiles'; tab_id: string; x: number; y: number; w: number; h: number; zoom: number }
  | { type: 'select_tool'; tool: string }
  | { type: 'get_state' }
  | { type: 'screenshot' }
  | { type: 'close' };

// -----------------------------------------------------------------------------
// Engine state (as returned by /api/state)
// -----------------------------------------------------------------------------

export interface TabInfo {
  tab_id: string;
  has_image: boolean;
  width?: number;
  height?: number;
}

export interface EngineState {
  tabs: TabInfo[];
}

// -----------------------------------------------------------------------------
// UI state derived from engine events
// -----------------------------------------------------------------------------

export interface UITab {
  id: string;
  name: string;
  color: string;
  modified: boolean;
  hasImage?: boolean;
  width?: number;
  height?: number;
}

export interface UIState {
  tabs: UITab[];
  activeTabId: string | null;
  activeTool: string;
  zoom: number;
  pan: { x: number; y: number };
}
