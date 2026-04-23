import { useEffect, useRef, useState, useCallback } from 'react';
import type { EngineEvent, EngineCommand, UIState } from './types';

const ENGINE_WS = 'ws://127.0.0.1:8080';

interface UseEngineEventsReturn {
  state: UIState;
  sendCommand: (cmd: EngineCommand) => void;
  connected: boolean;
  error: string | null;
  createTab: () => void;
  createTabAndOpen: (path: string) => void;
  closeTab: (tabId: string) => void;
  activateTab: (tabId: string) => void;
  openFile: (tabId: string, path: string) => void;
  selectTool: (tool: string) => void;
}

export function useEngineEvents(): UseEngineEventsReturn {
  const [state, setState] = useState<UIState>({
    tabs: [],
    activeTabId: null,
    activeTool: 'move',
    zoom: 100,
    pan: { x: 0, y: 0 },
  });
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingOpenPathRef = useRef<string | null>(null);
  const shouldReconnectRef = useRef(true);

  // Generate a deterministic color from tab ID
  const tabColor = (id: string): string => {
    const hash = id.split('').reduce((acc, char) => char.charCodeAt(0) + ((acc << 5) - acc), 0);
    const hue = Math.abs(hash % 360);
    return `hsl(${hue}, 60%, 65%)`;
  };

  // Connect to WebSocket
  const connect = useCallback(() => {
    if (
      wsRef.current?.readyState === WebSocket.OPEN ||
      wsRef.current?.readyState === WebSocket.CONNECTING
    ) {
      return;
    }

    const ws = new WebSocket(`${ENGINE_WS}/ws`);
    wsRef.current = ws;

    ws.onopen = () => {
      console.log('Connected to engine');
      setConnected(true);
      setError(null);
      // Request initial state
      ws.send(JSON.stringify({ type: 'get_state' }));
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as EngineEvent;
        console.log('Engine event:', data);
        handleEvent(data);
      } catch (err) {
        console.error('Failed to parse engine event:', err);
      }
    };

    ws.onerror = (event) => {
      console.error('WebSocket error:', event);
      setError('Connection error');
    };

    ws.onclose = () => {
      console.log('WebSocket closed');
      setConnected(false);
      if (!shouldReconnectRef.current) return;
      if (wsRef.current !== ws) return;
      // Attempt reconnect after 2 seconds
      if (reconnectTimeoutRef.current) clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = setTimeout(() => {
        connect();
      }, 2000);
    };
  }, []);

  // Send command to engine
  const sendCommand = useCallback((cmd: EngineCommand) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(cmd));
    } else {
      console.warn('WebSocket not connected, command dropped:', cmd);
    }
  }, []);

  // Convenience methods
  const createTab = useCallback(() => sendCommand({ type: 'create_tab' }), [sendCommand]);
  const createTabAndOpen = useCallback((path: string) => {
    pendingOpenPathRef.current = path;
    sendCommand({ type: 'create_tab' });
  }, [sendCommand]);
  const closeTab = useCallback((tabId: string) => sendCommand({ type: 'close_tab', tab_id: tabId }), [sendCommand]);
  const activateTab = useCallback((tabId: string) => sendCommand({ type: 'activate_tab', tab_id: tabId }), [sendCommand]);
  const openFile = useCallback((tabId: string, path: string) => sendCommand({ type: 'open_file', tab_id: tabId, path }), [sendCommand]);
  const selectTool = useCallback((tool: string) => sendCommand({ type: 'select_tool', tool }), [sendCommand]);

  // Handle incoming events
  const handleEvent = useCallback((event: EngineEvent) => {
    switch (event.type) {
      case 'tab_created':
        if (pendingOpenPathRef.current) {
          sendCommand({ type: 'activate_tab', tab_id: event.tab_id });
          sendCommand({ type: 'open_file', tab_id: event.tab_id, path: pendingOpenPathRef.current });
          pendingOpenPathRef.current = null;
        } else {
          sendCommand({ type: 'activate_tab', tab_id: event.tab_id });
        }
        setState(prev => ({
          ...prev,
          tabs: [...prev.tabs, {
            id: event.tab_id,
            name: event.name || 'Untitled',
            color: tabColor(event.tab_id),
            modified: false,
            hasImage: false,
          }],
        }));
        break;

      case 'tab_closed':
        setState(prev => ({
          ...prev,
          tabs: prev.tabs.filter(t => t.id !== event.tab_id),
          activeTabId: prev.activeTabId === event.tab_id
            ? (prev.tabs.length > 1 ? prev.tabs.find(t => t.id !== event.tab_id)?.id ?? null : null)
            : prev.activeTabId,
        }));
        break;

      case 'tab_activated':
        setState(prev => ({ ...prev, activeTabId: event.tab_id }));
        break;

      case 'image_loaded':
        setState(prev => ({
          ...prev,
          tabs: prev.tabs.map(t => t.id === event.tab_id ? {
            ...t,
            hasImage: true,
            width: event.width,
            height: event.height,
            name: `${event.width}x${event.height}`,
          } : t),
        }));
        break;

      case 'image_closed':
        setState(prev => ({
          ...prev,
          tabs: prev.tabs.map(t => t.id === event.tab_id ? {
            ...t,
            hasImage: false,
            width: undefined,
            height: undefined,
          } : t),
        }));
        break;

      case 'tool_changed':
        setState(prev => ({ ...prev, activeTool: event.tool }));
        break;

      case 'viewport_updated':
        setState(prev => {
          if (event.tab_id !== prev.activeTabId) return prev;
          return {
            ...prev,
            zoom: event.zoom * 100,
            pan: { x: event.pan_x, y: event.pan_y },
          };
        });
        break;

      case 'error':
        console.error('Engine error:', event.message);
        setError(event.message);
        break;

      // tile_data, tiles_complete, tiles_dirty are handled by Viewport component
      default:
        break;
    }
  }, [sendCommand]);

  // Initial connection and cleanup
  useEffect(() => {
    shouldReconnectRef.current = true;
    connect();
    return () => {
      shouldReconnectRef.current = false;
      if (reconnectTimeoutRef.current) clearTimeout(reconnectTimeoutRef.current);
      wsRef.current?.close();
    };
  }, [connect]);

  return {
    state,
    sendCommand,
    connected,
    error,
    createTab,
    createTabAndOpen,
    closeTab,
    activateTab,
    openFile,
    selectTool,
  };
}
