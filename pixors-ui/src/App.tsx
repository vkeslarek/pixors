import { useCallback, useEffect, useState } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { MenuBar, TabBar } from './components/MenuBar'
import { ActivityBar, type Workspace } from './components/ActivityBar'
import { Toolbar } from './components/Toolbar'
import { Viewport } from './components/Viewport'
import { Sidebar } from './components/Sidebar'
import { StatusBar } from './components/StatusBar'
import { useEngineEvents } from './engine/useEngineEvents'
import type { Layer, Adjustment, MousePos } from './types'
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
  // Engine connection and state
  const {
    state: engineState,
    connected,
    error,
    createTab,
    createTabAndOpen,
    closeTab: engineCloseTab,
    activateTab,
    openFile,
    selectTool,
  } = useEngineEvents();

  // UI-specific state (mock, will be replaced by engine in later phases)
  const [workspace, setWorkspace]       = useState<Workspace>('editor');
  const [layers, setLayers]             = useState<Layer[]>(INIT_LAYERS);
  const [activeLayerId, setActiveLayerId] = useState('4');
  const [adjustments, setAdjustments]   = useState<Adjustment[]>(INIT_ADJ);
  const [mousePos, setMousePos]         = useState<MousePos>({ x: 0, y: 0 });
  const [panelsOpen, setPanelsOpen]     = useState({ hist: true, props: true, adj: true, layers: true });

  useEffect(() => {
    if (connected) {
      createTabAndOpen('example1.png');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected])
  
  useEffect(() => {
    const toolMap: Record<string, string> = {
      v:'move', m:'select', l:'lasso', w:'wand', c:'crop', i:'eyedropper',
      b:'brush', e:'eraser', j:'heal', g:'gradient', t:'text', u:'shape', h:'hand', z:'zoom',
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      const tool = toolMap[e.key.toLowerCase()];
      if (tool) { selectTool(tool); e.preventDefault(); }
      if (e.ctrlKey && e.key === 'o') {
        e.preventDefault();
        const path = 'example1.png';
        if (engineState.activeTabId) {
          openFile(engineState.activeTabId, path);
        } else {
          createTabAndOpen(path);
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [createTabAndOpen, engineState.activeTabId, openFile, selectTool]);

  // Layer mutations (mock)
  const toggleVisibility = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,visible:!l.visible} : l));
  const toggleLock       = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,locked:!l.locked} : l));
  const deleteLayer      = (id: string) => {
    setLayers(ls => ls.filter(l => l.id !== id));
    setActiveLayerId(id === activeLayerId ? (layers.find(l=>l.id!==id)?.id ?? '') : activeLayerId);
  };
  const addLayer = () => {
    const l: Layer = { id: Date.now().toString(), name: `Layer ${layers.length+1}`, type:'image', visible:true, locked:false, opacity:100, blendMode:'Normal', color:'#aaa' };
    setLayers(ls => [l, ...ls]); setActiveLayerId(l.id);
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

  // Tab mutations (forward to engine)
  const handleTabClick = (tabId: string) => {
    activateTab(tabId);
  };
  const handleTabClose = (tabId: string) => {
    engineCloseTab(tabId);
  };
  const handleTabAdd = () => {
    createTab();
  };
  const handleOpenFile = useCallback(() => {
    const path = window.prompt('Path to image (engine filesystem):', 'example1.png');
    if (!path) return;
    if (engineState.activeTabId) {
      openFile(engineState.activeTabId, path);
      return;
    }
    createTabAndOpen(path);
  }, [createTabAndOpen, engineState.activeTabId, openFile]);

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar
          activeTabName={engineState.tabs.find(t => t.id === engineState.activeTabId)?.name}
          onOpenFile={handleOpenFile}
          onExport={() => console.log('export')}
        />
        <div className="workspace">
          <ActivityBar workspace={workspace} onWorkspaceChange={setWorkspace} />
          <Toolbar activeTool={engineState.activeTool} onToolSelect={selectTool} />
          <div className="canvas-column">
            <TabBar
              tabs={engineState.tabs}
              activeTabId={engineState.activeTabId}
              onTabClick={handleTabClick}
              onTabClose={handleTabClose}
              onTabAdd={handleTabAdd}
            />
            <Viewport
              activeTool={engineState.activeTool}
              zoom={engineState.zoom}
              onMouseMove={(x,y)=>setMousePos({x,y})}
              tabId={engineState.activeTabId}
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
        <StatusBar
          activeTool={engineState.activeTool}
          mousePos={mousePos}
          zoom={engineState.zoom}
          layerCount={layers.length}
          connected={connected}
          error={error}
        />
      </div>
    </Tooltip.Provider>
  );
}
