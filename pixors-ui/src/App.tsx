import { useEffect, useState } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { MenuBar, TabBar } from './components/MenuBar'
import { ActivityBar, type Workspace } from './components/ActivityBar'
import { Toolbar, TOOLS } from './components/Toolbar'
import { Viewport } from './components/Viewport'
import { Sidebar } from './components/Sidebar'
import { StatusBar } from './components/StatusBar'
import type { Layer, Adjustment, Tab, MousePos } from './types'
import './App.css'

// ── Initial state ─────────────────────────────────────────────────────────
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

const INIT_TABS: Tab[] = [
  { id: 't1', name: 'portrait-edit.psd', color: '#6b8fa8', modified: true },
  { id: 't2', name: 'landscape.psd',     color: '#8ba86b', modified: false },
  { id: 't3', name: 'product-shot.psd',  color: '#a86b8b', modified: false },
]

// ── App ───────────────────────────────────────────────────────────────────
export default function App() {
  const [activeTool, setActiveTool]     = useState('brush')
  const [workspace, setWorkspace]       = useState<Workspace>('editor')
  const [layers, setLayers]             = useState<Layer[]>(INIT_LAYERS)
  const [activeLayerId, setActiveLayerId] = useState('4')
  const [adjustments, setAdjustments]   = useState<Adjustment[]>(INIT_ADJ)
  const [zoom, setZoom]                 = useState(67)
  const [mousePos, setMousePos]         = useState<MousePos>({ x: 0, y: 0 })
  const [tabs, setTabs]                 = useState<Tab[]>(INIT_TABS)
  const [activeTabId, setActiveTabId]   = useState('t1')
  const [panelsOpen, setPanelsOpen]     = useState({ hist: true, props: true, adj: true, layers: true })

  // Global keyboard shortcuts
  useEffect(() => {
    const toolMap: Record<string, string> = {
      v:'move', m:'select', l:'lasso', w:'wand', c:'crop', i:'eyedropper',
      b:'brush', e:'eraser', j:'heal', g:'gradient', t:'text', u:'shape', h:'hand', z:'zoom',
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return
      const tool = toolMap[e.key.toLowerCase()]
      if (tool) { setActiveTool(tool); e.preventDefault() }
      if (e.key === '+' || e.key === '=') { setZoom(z => Math.min(z + 10, 400)); e.preventDefault() }
      if (e.key === '-') { setZoom(z => Math.max(z - 10, 10)); e.preventDefault() }
      if (e.key === '0') { setZoom(100); e.preventDefault() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // Layer mutations
  const toggleVisibility = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,visible:!l.visible} : l))
  const toggleLock       = (id: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,locked:!l.locked} : l))
  const deleteLayer      = (id: string) => {
    setLayers(ls => ls.filter(l => l.id !== id))
    setActiveLayerId(id === activeLayerId ? (layers.find(l=>l.id!==id)?.id ?? '') : activeLayerId)
  }
  const addLayer = () => {
    const l: Layer = { id: Date.now().toString(), name: `Layer ${layers.length+1}`, type:'image', visible:true, locked:false, opacity:100, blendMode:'Normal', color:'#aaa' }
    setLayers(ls => [l, ...ls]); setActiveLayerId(l.id)
  }
  const duplicateLayer = () => {
    const src = layers.find(l => l.id === activeLayerId)
    if (!src) return
    const dup: Layer = { ...src, id: Date.now().toString(), name: src.name + ' copy' }
    setLayers(ls => [dup, ...ls]); setActiveLayerId(dup.id)
  }
  const changeBlend   = (id: string, mode: string) => setLayers(ls => ls.map(l => l.id===id ? {...l,blendMode:mode} : l))
  const changeOpacity = (id: string, v: number)    => setLayers(ls => ls.map(l => l.id===id ? {...l,opacity:v} : l))

  // Adjustment mutations
  const changeAdj = (id: string, v: number) => setAdjustments(as => as.map(a => a.id===id ? {...a,value:v} : a))
  const resetAdj  = () => setAdjustments(INIT_ADJ)

  // Tab mutations
  const closeTab = (id: string) => {
    const remaining = tabs.filter(t => t.id !== id)
    setTabs(remaining)
    if (activeTabId === id && remaining.length) setActiveTabId(remaining[0].id)
  }
  const addTab = () => {
    const id = 't' + Date.now()
    setTabs(ts => [...ts, { id, name: 'untitled.psd', color: '#6b8fa8', modified: false }])
    setActiveTabId(id)
  }

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar
          activeTabName={tabs.find(t => t.id === activeTabId)?.name}
          onOpenFile={() => console.log('open')} onExport={() => console.log('export')}
        />
        <div className="workspace">
          <ActivityBar workspace={workspace} onWorkspaceChange={setWorkspace} />
          <Toolbar activeTool={activeTool} onToolSelect={setActiveTool} />
          <div className="canvas-column">
            <TabBar
              tabs={tabs} activeTabId={activeTabId}
              onTabClick={setActiveTabId} onTabClose={closeTab} onTabAdd={addTab}
            />
            <Viewport activeTool={activeTool} zoom={zoom} onMouseMove={(x,y)=>setMousePos({x,y})} />
          </div>
          <Sidebar
            layers={layers} activeLayerId={activeLayerId} adjustments={adjustments}
            panelsOpen={panelsOpen} onPanelToggle={key => setPanelsOpen(p => ({...p,[key]:!p[key as keyof typeof p]}))}
            onLayerClick={setActiveLayerId} onToggleVisibility={toggleVisibility} onToggleLock={toggleLock}
            onDeleteLayer={deleteLayer} onAddLayer={addLayer} onDuplicateLayer={duplicateLayer}
            onBlendChange={changeBlend} onLayerOpacityChange={changeOpacity}
            onAdjChange={changeAdj} onAdjReset={resetAdj}
          />
        </div>
        <StatusBar activeTool={activeTool} mousePos={mousePos} zoom={zoom} layerCount={layers.length} />
      </div>
    </Tooltip.Provider>
  )
}