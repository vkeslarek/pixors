import { useEffect, useState, useRef } from 'react';
import { engineClient } from './client';
import type { EngineEvent, EngineCommand, TabData, UITab } from './types';

export function useEngineClient() {
  useEffect(() => {
    engineClient.connect();
  }, []);
  return engineClient;
}

export function useEngineEvent<T extends EngineEvent['type']>(
  type: T,
  handler: (event: Extract<EngineEvent, { type: T }>) => void
) {
  const savedHandler = useRef(handler);
  
  useEffect(() => {
    savedHandler.current = handler;
  }, [handler]);

  useEffect(() => {
    return engineClient.on(type, (e) => savedHandler.current(e as any));
  }, [type]);
}

export function useEngineConnection() {
  const [connected, setConnected] = useState(engineClient.connected);
  useEffect(() => {
    setConnected(engineClient.connected);
    return engineClient.onConnection(setConnected);
  }, []);
  return connected;
}

export function useEngineSession() {
  const [sessionId] = useState(engineClient.sessionId);
  const [tabData, setTabData] = useState<TabData[]>([]);
  const [status, setStatus] = useState<'Connected' | 'Disconnected'>('Connected');

  useEngineEvent('session_state', (msg) => {
    setTabData(msg.tabs);
    setStatus(msg.status);
  });

  return { sessionId, tabData, status };
}

const TAB_COLORS = ['#ff4d4d', '#4dff4d', '#4d4dff', '#ffff4d', '#ff4dff', '#6ffff'];

export function useEngineTabs() {
  const [tabs, setTabs] = useState<UITab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);

  useEngineEvent('session_state', (msg) => {
    setTabs(msg.tabs.map((td, i) => ({
      id: td.id,
      name: td.name,
      color: TAB_COLORS[i % TAB_COLORS.length],
      modified: false,
      hasImage: td.has_image,
      width: td.width,
      height: td.height,
    })));
  });

  useEngineEvent('tab_created', (msg) => {
    setTabs(prev => {
      if (prev.find(t => t.id === msg.tab_id)) return prev;
      return [...prev, {
        id: msg.tab_id,
        name: msg.name,
        color: TAB_COLORS[prev.length % TAB_COLORS.length],
        modified: false
      }];
    });
  });

  useEngineEvent('tab_closed', (msg) => {
    setTabs(prev => prev.filter(t => t.id !== msg.tab_id));
    setActiveTabId(prev => (prev === msg.tab_id ? null : prev)); // Fallback selection logic should ideally be handled by checking length
  });

  useEngineEvent('tab_activated', (msg) => {
    setActiveTabId(msg.tab_id);
  });

  useEngineEvent('image_loaded', (msg) => {
    setTabs(prev => prev.map(t => 
      t.id === msg.tab_id ? { ...t, hasImage: true, width: msg.width, height: msg.height } : t
    ));
  });

  // Ensure active tab fallback
  useEffect(() => {
    if (activeTabId && !tabs.find(t => t.id === activeTabId)) {
      setActiveTabId(tabs.length > 0 ? tabs[tabs.length - 1].id : null);
    }
  }, [tabs, activeTabId]);

  return { tabs, activeTabId };
}

export function useEngineTools() {
  const [activeTool, setActiveTool] = useState('Select');
  useEngineEvent('tool_changed', (msg) => {
    setActiveTool(msg.tool);
  });
  return { activeTool };
}

export function useEngineViewportState(tabId: string | null) {
  const [zoom, setZoom] = useState(1.0);
  const [pan, setPan] = useState({ x: 0, y: 0 });

  useEngineEvent('viewport_updated', (msg) => {
    if (msg.tab_id === tabId) {
      setZoom(msg.zoom);
      setPan({ x: msg.pan_x, y: msg.pan_y });
    }
  });

  return { zoom, pan };
}

export function useLoadingProgress(tabId: string | null) {
  const [progress, setProgress] = useState<{ active: boolean; percent: number }>({
    active: false,
    percent: 0,
  });

  useEngineEvent('image_load_progress', (msg) => {
    if (msg.tab_id !== tabId) return;
    setProgress({ active: true, percent: msg.percent });
  });

  useEngineEvent('image_loaded', (msg) => {
    if (msg.tab_id !== tabId) return;
    setProgress({ active: false, percent: 100 });
  });

  useEngineEvent('image_closed', (msg) => {
    if (msg.tab_id !== tabId) return;
    setProgress({ active: false, percent: 0 });
  });

  return progress;
}

export function useEngineCommands() {
  return {
    sendCommand: (cmd: EngineCommand) => engineClient.sendCommand(cmd),
    createTab: () => engineClient.sendCommand({ type: 'create_tab' }),
    closeTab: (tabId: string) => engineClient.sendCommand({ type: 'close_tab', tab_id: tabId }),
    activateTab: (tabId: string) => engineClient.sendCommand({ type: 'activate_tab', tab_id: tabId }),
    selectTool: (tool: string) => engineClient.sendCommand({ type: 'select_tool', tool }),
    createTabAndOpen: (path: string) => {
      // Create tab, wait for tab_created, then activate & open_file
      const onTabCreated = (msg: Extract<EngineEvent, { type: 'tab_created' }>) => {
        engineClient.sendCommand({ type: 'activate_tab', tab_id: msg.tab_id });
        engineClient.sendCommand({ type: 'open_file', tab_id: msg.tab_id, path });
        engineClient.off('tab_created', onTabCreated);
      };
      engineClient.on('tab_created', onTabCreated);
      engineClient.sendCommand({ type: 'create_tab' });
    }
  };
}
