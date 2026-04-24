import { useState, useEffect, useCallback, useRef } from 'react';
import { decode, encode } from '@msgpack/msgpack';
import type { EngineEvent, EngineCommand, UIState } from './types';
import { PixorsViewport } from 'pixors-viewport';

// Fallback colors for tabs
const TAB_COLORS = ['#ff4d4d', '#4dff4d', '#4d4dff', '#ffff4d', '#ff4dff', '#6ffff'];

// UUID wire format helpers (server sends/expects 16-byte binary)
function binToHex(b: Uint8Array): string {
  return Array.from(b).map(x => x.toString(16).padStart(2, '0')).join('');
}
function hexToBin(h: string): Uint8Array {
  const a = new Uint8Array(h.length / 2);
  for (let i = 0; i < a.length; i++) a[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
  return a;
}

interface UseEngineReturn {
  state: UIState;
  sendCommand: (cmd: EngineCommand) => void;
  connected: boolean;
  error: string | null;

  // Convenience methods
  createTab: () => void;
  closeTab: (tabId: string) => void;
  activateTab: (tabId: string) => void;
  openFile: (tabId: string, path: string) => void;
  selectTool: (tool: string) => void;
  createTabAndOpen: (path: string) => void;
  requestTiles: (tabId: string, x: number, y: number, w: number, h: number, zoom: number) => void;
}

// TLV message type tags — must match server constants
const MSG_EVENT = 0x00;
const MSG_TILE = 0x01;
const MSG_TILES_COMPLETE = 0x02;

export function useEngine(
  viewportRef: React.RefObject<PixorsViewport | null>
): UseEngineReturn {
  const [state, setState] = useState<UIState>({
    tabs: [],
    activeTabId: null,
    activeTool: 'crop',
    zoom: 1.0,
    pan: { x: 0, y: 0 }
  });

  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ws = useRef<WebSocket | null>(null);
  const pendingOpenPathRef = useRef<string | null>(null);

  // Ref to latest state for use in async callbacks
  const prevStateRef = useRef(state);
  prevStateRef.current = state;

  const connect = useCallback(() => {
    if (ws.current?.readyState === WebSocket.OPEN || ws.current?.readyState === WebSocket.CONNECTING) return;

    const socket = new WebSocket(`ws://127.0.0.1:8080/ws`);
    ws.current = socket;
    socket.binaryType = 'arraybuffer';

    socket.onopen = () => {
      console.log('Engine connected');
      setConnected(true);
      setError(null);
      socket.send(encode({ type: 'get_state' }));
    };

    socket.onclose = () => {
      console.log('Engine disconnected');
      setConnected(false);
      ws.current = null;
      setTimeout(connect, 1000);
    };

    socket.onerror = (e) => {
      console.error('WebSocket error:', e);
      setError('Connection error');
    };

    socket.onmessage = (event) => {
      console.log('WS message received, type:', typeof event.data, 'size:', event.data.byteLength || event.data.length);
      const buf = event.data as ArrayBuffer;
      const view = new DataView(buf);
      const type = view.getUint8(0);
      const payloadLen = view.getUint32(1, true);
      console.log('WS binary: type=%d payloadLen=%d totalLen=%d', type, payloadLen, buf.byteLength);

      switch (type) {
        case MSG_EVENT: {
          const payload = new Uint8Array(buf, 5, payloadLen);
          let msg: EngineEvent;
          try {
            const raw = decode(payload) as Record<string, unknown>;
            // Server sends UUID as 16-byte binary; normalize to hex string for internal use
            if (raw.tab_id instanceof Uint8Array) raw.tab_id = binToHex(raw.tab_id);
            msg = raw as unknown as EngineEvent;
          } catch (err) {
            console.error('Failed to decode event payload:', err, 'len=', payloadLen);
            break;
          }
          const hexBytes = Array.from(new Uint8Array(payload.buffer, payload.byteOffset, Math.min(payloadLen, 32))).map(b => b.toString(16).padStart(2, '0')).join(' ');
          console.log('Engine event keys:', Object.keys(msg), 'type:', typeof msg, 'isMap:', msg instanceof Map, 'hex:', hexBytes);

          // Side-effects outside setState
          if (msg.type === 'tab_created') {
            if (pendingOpenPathRef.current) {
              const path = pendingOpenPathRef.current;
              pendingOpenPathRef.current = null;
              setTimeout(() => {
                if (ws.current?.readyState === WebSocket.OPEN) {
                  const idBin = hexToBin(msg.tab_id);
                  ws.current.send(encode({ type: 'activate_tab', tab_id: idBin }));
                  ws.current.send(encode({ type: 'open_file', tab_id: idBin, path }));
                }
              }, 0);
            }
          } else if (msg.type === 'tab_activated') {
            // handled via useEffect
          } else if (msg.type === 'image_loaded') {
            // handled via useEffect
          } else if (msg.type === 'tiles_dirty') {
            // Tile was modified → re-request visible tiles for the affected tab
            const tab = (prevStateRef.current ?? state).tabs.find(t => t.id === msg.tab_id);
            if (tab && ws.current?.readyState === WebSocket.OPEN) {
              const idBin = hexToBin(msg.tab_id);
              ws.current.send(encode({
                type: 'request_tiles',
                tab_id: idBin,
                x: (prevStateRef.current ?? state).pan.x,
                y: (prevStateRef.current ?? state).pan.y,
                w: window.innerWidth - 300, // viewport width approximation
                h: window.innerHeight - 100, // viewport height approximation
                zoom: (prevStateRef.current ?? state).zoom,
              }));
            }
          }

          setState(prev => {
            const next = { ...prev };

            switch (msg.type) {
              case 'tab_created':
                if (!next.tabs.find(t => t.id === msg.tab_id)) {
                  next.tabs = [...next.tabs, {
                    id: msg.tab_id,
                    name: msg.name,
                    color: TAB_COLORS[next.tabs.length % TAB_COLORS.length],
                    modified: false
                  }];
                }
                break;

              case 'tab_closed':
                next.tabs = next.tabs.filter(t => t.id !== msg.tab_id);
                if (next.activeTabId === msg.tab_id) {
                  next.activeTabId = next.tabs.length > 0 ? next.tabs[next.tabs.length - 1].id : null;
                }
                break;

              case 'tab_activated':
                next.activeTabId = msg.tab_id;
                break;

              case 'image_loaded':
                next.tabs = next.tabs.map(t =>
                  t.id === msg.tab_id
                    ? { ...t, hasImage: true, width: msg.width, height: msg.height }
                    : t
                );
                break;

              case 'tool_changed':
                next.activeTool = msg.tool;
                break;

              case 'viewport_updated':
                if (next.activeTabId === msg.tab_id) {
                  next.zoom = msg.zoom;
                  next.pan = { x: msg.pan_x, y: msg.pan_y };
                }
                break;
            }
            return next;
          });
          break;
        }

        case MSG_TILE: {
          // 36-byte tile header + RGBA8 pixel data
          const payload = new Uint8Array(buf, 5, payloadLen);
          const tileView = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
          const x = tileView.getUint32(0, true);
          const y = tileView.getUint32(4, true);
          const w = tileView.getUint32(8, true);
          const h = tileView.getUint32(12, true);
          const mip = tileView.getUint32(16, true);
          // bytes 20-35: tab_id UUID (16 bytes) — ignored, we write by position

          if (viewportRef.current) {
            try {
              const pixelData = new Uint8Array(payload.buffer, payload.byteOffset + 36, payloadLen - 36);
              viewportRef.current.write_tile(x, y, w, h, mip, pixelData);
            } catch (err) {
              console.error('WASM write_tile failed:', err);
            }
          }
          break;
        }

        case MSG_TILES_COMPLETE:
          // No action needed — render loop handles display
          break;

        default:
          console.warn('Unknown binary message type:', type);
      }
    };
  }, [viewportRef]);

  const createdTextureTabIdRef = useRef<string | null>(null);

  useEffect(() => {
    const activeTab = state.tabs.find(t => t.id === state.activeTabId);
    if (activeTab && activeTab.hasImage && activeTab.width && activeTab.height) {
      if (createdTextureTabIdRef.current !== state.activeTabId) {
        if (viewportRef.current) {
          try {
            viewportRef.current.create_empty_texture(activeTab.width, activeTab.height);
            createdTextureTabIdRef.current = state.activeTabId;
          } catch (err) {
            console.error('Failed to create empty texture:', err);
          }
        }
      }
    }
  }, [state.activeTabId, state.tabs, viewportRef]);

  useEffect(() => {
    connect();
    return () => {
      if (ws.current) {
        ws.current.close();
        ws.current = null;
      }
    };
  }, [connect]);

  const sendCommand = useCallback((cmd: EngineCommand) => {
    if (ws.current?.readyState === WebSocket.OPEN) {
      // Convert hex tab_id string back to 16-byte binary for server (expects Uuid as bin8)
      const wire = { ...cmd } as Record<string, unknown>;
      if (typeof wire.tab_id === 'string' && wire.tab_id.length === 32) {
        wire.tab_id = hexToBin(wire.tab_id);
      }
      ws.current.send(encode(wire));
    } else {
      console.warn('WebSocket not open, cannot send command:', cmd);
    }
  }, []);

  const createTab = useCallback(() => sendCommand({ type: 'create_tab' }), [sendCommand]);
  const closeTab = useCallback((tabId: string) => sendCommand({ type: 'close_tab', tab_id: tabId }), [sendCommand]);
  const activateTab = useCallback((tabId: string) => sendCommand({ type: 'activate_tab', tab_id: tabId }), [sendCommand]);
  const openFile = useCallback((tabId: string, path: string) => sendCommand({ type: 'open_file', tab_id: tabId, path }), [sendCommand]);
  const selectTool = useCallback((tool: string) => sendCommand({ type: 'select_tool', tool }), [sendCommand]);

  const createTabAndOpen = useCallback((path: string) => {
    pendingOpenPathRef.current = path;
    createTab();
  }, [createTab]);

  const requestTiles = useCallback((tabId: string, x: number, y: number, w: number, h: number, zoom: number) => {
    sendCommand({ type: 'request_tiles', tab_id: tabId, x, y, w, h, zoom });
  }, [sendCommand]);

  return {
    state,
    connected,
    error,
    sendCommand,
    createTab,
    closeTab,
    activateTab,
    openFile,
    selectTool,
    createTabAndOpen,
    requestTiles,
  };
}
