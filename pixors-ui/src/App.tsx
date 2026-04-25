import { useEffect, useRef, useState } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { MenuBar, TabBar } from './components/MenuBar'
import { ActivityBar, type Workspace } from './components/ActivityBar'
import { Toolbar } from './components/Toolbar'
import { Viewport } from './components/Viewport'
import { Sidebar } from './components/Sidebar'
import { StatusBar } from './components/StatusBar'
import { ProgressBar } from './components/ProgressBar'
import { useEngineConnection, useEngineTabs, useEngineTools, useEngineCommands, useEngineViewportState, useEngineClient, useLoadingProgress } from './engine'
import type { Layer, Adjustment } from './types'
import './App.css'

// ── Initial state (mock, will be replaced by engine) ─────────────────────
const INIT_LAYERS: Layer[] = [
  { id: '1', name: 'Background',    type: 'image',      visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#888' },
  { id: '2', name: 'Portrait',      type: 'image',      visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#6fc' },
  { id: '3', name: 'Texture',       type: 'image',      visible: true, locked: false, opacity: 50,  blendMode: 'Overlay', color: '#fc6' },
  { id: '4', name: 'Curves 1',      type: 'adjustment', visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#6cf' },
  { id: '5', name: 'Color Overlay', type: 'adjustment', visible: true, locked: false, opacity: 100, blendMode: 'Color',   color: '#c6f' },
]

const INIT_ADJ: Adjustment[] = [
  { id: 'exposure',  label: 'Exposure',    min: -5,   max: 5,   step: 0.01, value: 0 },
  { id: 'contrast',  label: 'Contrast',    min: -100, max: 100, step: 1,    value: 0 },
  { id: 'highlights',label: 'Highlights',  min: -100, max: 100, step: 1,    value: 0 },
  { id: 'shadows',   label: 'Shadows',     min: -100, max: 100, step: 1,    value: 0 },
  { id: 'temp',      label: 'Temperature', min: -100, max: 100, step: 1,    value: 0 },
  { id: 'vibrance',  label: 'Vibrance',    min: -100, max: 100, step: 1,    value: 0 },
  { id: 'saturation',label: 'Saturation',  min: -100, max: 100, step: 1,    value: 0 },
  { id: 'clarity',   label: 'Clarity',     min: -100, max: 100, step: 1,    value: 0 },
  { id: 'sharpening',label: 'Sharpening',  min: 0,    max: 150, step: 1,    value: 40 },
]

// ── App ───────────────────────────────────────────────────────────────────
export default function App() {
  // Engine hooks
  useEngineClient();
  const connected = useEngineConnection();
  const { tabs, activeTabId } = useEngineTabs();
  const { activeTool } = useEngineTools();
  const { zoom: engineZoom } = useEngineViewportState(activeTabId);
  const cmds = useEngineCommands();
  const loadingProgress = useLoadingProgress(activeTabId);

  // UI-specific state (mock, will be replaced by engine in later phases)
  const [workspace, setWorkspace]       = useState<Workspace>('editor');
  const [layers, setLayers]             = useState<Layer[]>(INIT_LAYERS);
  const [activeLayerId, setActiveLayerId] = useState('4');
  const [adjustments, setAdjustments]   = useState<Adjustment[]>(INIT_ADJ);
  const [panelsOpen, setPanelsOpen]     = useState({ hist: true, props: true, adj: true, layers: true });

  const initialLoadDone = useRef(false);
  useEffect(() => {
    if (connected && !initialLoadDone.current) {
      initialLoadDone.current = true;
      // Removed auto-opening of example1.png
      // cmds.createTabAndOpen('example1.png');
    }
  }, [connected, cmds])
  
  useEffect(() => {
    const toolMap: Record<string, string> = {
      v:'move', m:'select', l:'lasso', w:'wand', c:'crop', i:'eyedropper',
      b:'brush', e:'eraser', j:'heal', g:'gradient', t:'text', u:'shape', h:'hand', z:'zoom',
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      const tool = toolMap[e.key.toLowerCase()];
      if (tool) { cmds.selectTool(tool); e.preventDefault(); }
      if (e.ctrlKey && e.key === 'o') {
        e.preventDefault();
        cmds.sendCommand({ type: 'open_file_dialog', tab_id: activeTabId || undefined });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [cmds, activeTabId]);

  // Layer mutations (mock)
  const toggleVisibility = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,visible:!l.visible} : l));
  const toggleLock       = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,locked:!l.locked} : l));
  const deleteLayer      = (id: string) => {
    setLayers(ls => ls.filter(l => l.id !== id));
    setActiveLayerId(id === activeLayerId ? (layers.find(l=>l.id!==id)?.id ?? '') : activeLayerId);
  };
  const addLayer = () => {
    const nl = { id: Date.now().toString(), name: 'New Layer', type:'image', visible:true, locked:false, opacity:100, blendMode:'Normal', color:'#ccc' } as Layer;
    setLayers([nl, ...layers]);
    setActiveLayerId(nl.id);
  };
  const duplicateLayer = () => {
    const src = layers.find(l => l.id === activeLayerId);
    if (!src) return;
    const dup: Layer = { ...src, id: Date.now().toString(), name: src.name + ' copy' };
    setLayers(ls => [dup, ...ls]); setActiveLayerId(dup.id);
  };
  const changeBlend   = (id: string, mode: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,blendMode:mode} : l));
  const changeOpacity = (id: string, v: number)    => setLayers(ls => ls.map(l => l.id===id ? {...l,opacity:v} : l));

  // Adjustment mutations (mock)
  const changeAdj = (id: string, v: number) => setAdjustments(as => as.map(a => a.id===id ? {...a,value:v} : a));
  const resetAdj  = () => setAdjustments(INIT_ADJ);

  const handleTabClick = (id: string) => cmds.activateTab(id);
  const handleTabClose = (id: string) => cmds.closeTab(id);
  const handleTabAdd   = () => cmds.createTab();

  const handleOpenFileDialog = () => {
    cmds.sendCommand({ type: 'open_file_dialog', tab_id: activeTabId || undefined });
  };

  const activeTabObj = tabs.find(t => t.id === activeTabId);

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar
          activeTabName={activeTabObj?.name}
          onOpenFile={handleOpenFileDialog}
          onExport={() => console.log('export')}
        />
        <div className="workspace">
          <ActivityBar workspace={workspace} onWorkspaceChange={setWorkspace} />
          <Toolbar activeTool={activeTool} onToolSelect={cmds.selectTool} />
          <div className="canvas-column">
            <TabBar
              tabs={tabs}
              activeTabId={activeTabId}
              onTabClick={handleTabClick}
              onTabClose={handleTabClose}
              onTabAdd={handleTabAdd}
            />
            <Viewport
              tabId={activeTabId}
              imageWidth={activeTabObj?.width}
              imageHeight={activeTabObj?.height}
              activeTool={activeTool}
              connected={connected}
              sendCommand={cmds.sendCommand}
              onMouseMove={(x, y) => window.dispatchEvent(new CustomEvent('mouse_pos', { detail: { x, y } }))}
            />
          </div>
          <Sidebar
            layers={layers}
            activeLayerId={activeLayerId}
            adjustments={adjustments}
            panelsOpen={panelsOpen}
            onPanelToggle={key => setPanelsOpen(p => ({...p,[key]:!p[key as keyof typeof p]}))}
            onLayerClick={setActiveLayerId}
            onToggleVisibility={toggleVisibility}
            onToggleLock={toggleLock}
            onDeleteLayer={deleteLayer}
            onAddLayer={addLayer}
            onDuplicateLayer={duplicateLayer}
            onBlendChange={changeBlend}
            onLayerOpacityChange={changeOpacity}
            onAdjChange={changeAdj}
            onAdjReset={resetAdj}
          />
        </div>
        <ProgressBar percent={loadingProgress.percent} />
        <StatusBar
          activeTool={activeTool}
          zoom={engineZoom}
          layerCount={layers.length}
          connected={connected}
          error={null}
        />
      </div>
    </Tooltip.Provider>
  );
}
