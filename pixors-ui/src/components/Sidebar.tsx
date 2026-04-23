import { Eye, EyeOff, Lock, Trash2, Plus, Copy, Filter } from 'lucide-react'
import { ChevronRight } from 'lucide-react'
import type { Layer, Adjustment } from '../types'

// ── Histogram (fake RGB data) ─────────────────────────────────────────────
import { useEffect, useRef } from 'react'

function Histogram() {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')!
    const W = canvas.width = canvas.offsetWidth * devicePixelRatio
    const H = canvas.height = canvas.offsetHeight * devicePixelRatio
    ctx.clearRect(0, 0, W, H)
    const pts = 64
    const channels = [
      { color: 'rgba(255,80,80,0.6)', data: Array.from({length:pts},(_,i)=>Math.max(0,Math.exp(-((i-20)**2)/80)*0.7+Math.random()*0.1)) },
      { color: 'rgba(80,220,80,0.6)', data: Array.from({length:pts},(_,i)=>Math.max(0,Math.exp(-((i-32)**2)/120)*0.9+Math.random()*0.08)) },
      { color: 'rgba(80,160,255,0.6)', data: Array.from({length:pts},(_,i)=>Math.max(0,Math.exp(-((i-45)**2)/100)*0.6+Math.random()*0.12)) },
    ]
    ctx.globalCompositeOperation = 'screen'
    channels.forEach(ch => {
      const max = Math.max(...ch.data)
      ctx.beginPath(); ctx.moveTo(0, H)
      ch.data.forEach((v, i) => ctx.lineTo(i/(pts-1)*W, H-(v/max)*H*0.9))
      ctx.lineTo(W, H); ctx.closePath()
      ctx.fillStyle = ch.color; ctx.fill()
    })
  }, [])
  return <canvas ref={ref} className="histogram-canvas" style={{width:'100%',height:60}} />
}

// ── Properties panel ──────────────────────────────────────────────────────
interface PropertiesPanelProps {
  open: boolean
  onToggle: () => void
  activeLayer: Layer | undefined
  onOpacityChange: (v: number) => void
}

function PropertiesPanel({ open, onToggle, activeLayer, onOpacityChange }: PropertiesPanelProps) {
  return (
    <div className="panel">
      <div className="panel-header" onClick={onToggle}>
        <span className="panel-title">Properties</span>
        <ChevronRight className={`panel-chevron${open ? ' open' : ''}`} size={12} />
      </div>
      {open && (
        <div className="panel-body" style={{padding:'6px 0'}}>
          <div className="prop-grid">
             {[['W','900','px'],['H','600','px'],['X','0','px'],['Y','0','px']].map(([l,v,_u])=>(
              <div key={l} className="prop-field">
                <span className="prop-field-label">{l}</span>
                <input className="prop-input" defaultValue={v} />
              </div>
            ))}
          </div>
          <div className="prop-row">
            <span className="prop-label">Opacity</span>
            <input className="prop-input" type="number" min={0} max={100}
              value={activeLayer?.opacity ?? 100}
              onChange={e => onOpacityChange(Number(e.target.value))} />
            <span className="prop-unit">%</span>
          </div>
          <div className="prop-row">
            <span className="prop-label">Mode</span>
            <select className="blend-select" value={activeLayer?.blendMode ?? 'Normal'} onChange={()=>{}}>
              {['Normal','Dissolve','Multiply','Screen','Overlay','Soft Light','Hard Light','Color Dodge','Color Burn','Darken','Lighten','Difference','Exclusion','Hue','Saturation','Color','Luminosity'].map(m=><option key={m}>{m}</option>)}
            </select>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Adjustments panel ─────────────────────────────────────────────────────
const ADJ_SECTIONS = [
  { title: 'Light', items: [
    { id: 'exposure', label: 'Exposure', min: -5, max: 5, step: 0.01, default: 0 },
    { id: 'contrast', label: 'Contrast', min: -100, max: 100, step: 1, default: 0 },
    { id: 'highlights', label: 'Highlights', min: -100, max: 100, step: 1, default: 0 },
    { id: 'shadows', label: 'Shadows', min: -100, max: 100, step: 1, default: 0 },
  ]},
  { title: 'Color', items: [
    { id: 'temp', label: 'Temperature', min: -100, max: 100, step: 1, default: 0 },
    { id: 'vibrance', label: 'Vibrance', min: -100, max: 100, step: 1, default: 0 },
    { id: 'saturation', label: 'Saturation', min: -100, max: 100, step: 1, default: 0 },
  ]},
  { title: 'Detail', items: [
    { id: 'clarity', label: 'Clarity', min: -100, max: 100, step: 1, default: 0 },
    { id: 'sharpening', label: 'Sharpening', min: 0, max: 150, step: 1, default: 40 },
  ]},
]

interface AdjustmentsPanelProps {
  open: boolean
  onToggle: () => void
  adjustments: Adjustment[]
  onAdjChange: (id: string, v: number) => void
  onReset: () => void
}

import { RotateCcw } from 'lucide-react'

function AdjustmentsPanel({ open, onToggle, adjustments, onAdjChange, onReset }: AdjustmentsPanelProps) {
  return (
    <div className="panel expandable" style={{flex: open ? '1 1 0' : '0 0 auto'}}>
      <div className="panel-header" onClick={onToggle}>
        <span className="panel-title">Adjustments</span>
        <button className="icon-btn" onClick={e=>{e.stopPropagation();onReset()}} title="Reset all"><RotateCcw size={12}/></button>
        <ChevronRight className={`panel-chevron${open ? ' open' : ''}`} size={12} />
      </div>
      {open && (
        <div className="panel-body">
          {ADJ_SECTIONS.map(sec => (
            <div key={sec.title}>
              <div className="adj-section-title">{sec.title}</div>
              {sec.items.map(item => {
                const adj = adjustments.find(a => a.id === item.id)
                const val = adj?.value ?? item.default
                const fmt = item.step < 1 ? val.toFixed(2) : Math.round(val)
                return (
                  <div key={item.id} className="adj-row">
                    <div className="adj-header">
                      <span className="adj-name">{item.label}</span>
                      <span className="adj-value">{Number(fmt) > 0 ? '+' : ''}{fmt}</span>
                    </div>
                    <input type="range" className="adj-slider"
                      min={item.min} max={item.max} step={item.step} value={val}
                      style={{background:`linear-gradient(to right,var(--accent) 0%,var(--accent) ${((val-item.min)/(item.max-item.min))*100}%,var(--bg-active) ${((val-item.min)/(item.max-item.min))*100}%,var(--bg-active) 100%)`}}
                      onChange={e => onAdjChange(item.id, Number(e.target.value))} />
                  </div>
                )
              })}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// ── Layers panel ──────────────────────────────────────────────────────────
const BLEND_MODES = ['Normal','Dissolve','Multiply','Screen','Overlay','Soft Light','Hard Light','Color Dodge','Color Burn','Darken','Lighten','Difference','Exclusion','Hue','Saturation','Color','Luminosity']

interface LayersPanelProps {
  open: boolean
  onToggle: () => void
  layers: Layer[]
  activeLayerId: string
  onLayerClick: (id: string) => void
  onToggleVisibility: (id: string) => void
  onToggleLock: (id: string) => void
  onDelete: (id: string) => void
  onAdd: () => void
  onDuplicate: () => void
  onBlendChange: (id: string, mode: string) => void
  onOpacityChange: (id: string, v: number) => void
}

function LayersPanel({ open, onToggle, layers, activeLayerId, onLayerClick, onToggleVisibility, onToggleLock, onDelete, onAdd, onDuplicate, onBlendChange, onOpacityChange }: LayersPanelProps) {
  const active = layers.find(l => l.id === activeLayerId)
  return (
    <div className="panel expandable" style={{flex: open ? '2 1 0' : '0 0 auto', minHeight: open ? 120 : 0}}>
      <div className="panel-header" onClick={onToggle}>
        <span className="panel-title">Layers</span>
        <span className="panel-count">{layers.length}</span>
        <ChevronRight className={`panel-chevron${open ? ' open' : ''}`} size={12} />
      </div>
      {open && (
        <>
          <div className="layers-toolbar">
            <select className="blend-select" value={active?.blendMode ?? 'Normal'}
              onChange={e => onBlendChange(activeLayerId, e.target.value)}>
              {BLEND_MODES.map(m=><option key={m}>{m}</option>)}
            </select>
            <input className="opacity-input" type="number" min={0} max={100}
              value={active?.opacity ?? 100}
              onChange={e => onOpacityChange(activeLayerId, Number(e.target.value))} />
            <span className="pct">%</span>
          </div>
          <div className="panel-body" style={{flex:1}}>
            {layers.map(layer => (
              <div key={layer.id}
                className={`layer-item${layer.id === activeLayerId ? ' active' : ''}`}
                onClick={() => onLayerClick(layer.id)}
                style={{opacity: layer.visible ? 1 : 0.4}}>
                <div className="layer-thumb">
                  <div className="layer-thumb-checker" />
                  <div className="layer-thumb-color" style={{background:layer.color, opacity: layer.opacity/100*0.7}} />
                </div>
                <div className="layer-info">
                  <div className="layer-name">{layer.name}</div>
                  <div className="layer-type">{layer.type} · {layer.blendMode}</div>
                </div>
                <div className="layer-actions">
                  <button className="icon-btn" onClick={e=>{e.stopPropagation();onToggleVisibility(layer.id)}} title="Toggle visibility">
                    {layer.visible ? <Eye size={12}/> : <EyeOff size={12}/>}
                  </button>
                  <button className="icon-btn" onClick={e=>{e.stopPropagation();onToggleLock(layer.id)}} title="Lock" style={{color:layer.locked?'var(--accent)':undefined}}>
                    <Lock size={12}/>
                  </button>
                  <button className="icon-btn" onClick={e=>{e.stopPropagation();onDelete(layer.id)}} title="Delete">
                    <Trash2 size={12}/>
                  </button>
                </div>
              </div>
            ))}
          </div>
          <div className="layers-bottom">
            <button className="icon-btn" onClick={onAdd} title="Add layer"><Plus size={14}/></button>
            <button className="icon-btn" onClick={onDuplicate} title="Duplicate"><Copy size={14}/></button>
            <button className="icon-btn" title="Layer effects"><Filter size={14}/></button>
            <div style={{flex:1}}/>
            <button className="icon-btn" onClick={()=>onDelete(activeLayerId)} title="Delete" style={{color:'var(--danger)'}}><Trash2 size={14}/></button>
          </div>
        </>
      )}
    </div>
  )
}

// ── Sidebar (composes all panels) ─────────────────────────────────────────
interface SidebarProps {
  layers: Layer[]
  activeLayerId: string
  adjustments: Adjustment[]
  panelsOpen: Record<string, boolean>
  onPanelToggle: (key: string) => void
  onLayerClick: (id: string) => void
  onToggleVisibility: (id: string) => void
  onToggleLock: (id: string) => void
  onDeleteLayer: (id: string) => void
  onAddLayer: () => void
  onDuplicateLayer: () => void
  onBlendChange: (id: string, mode: string) => void
  onLayerOpacityChange: (id: string, v: number) => void
  onAdjChange: (id: string, v: number) => void
  onAdjReset: () => void
}

export function Sidebar({
  layers, activeLayerId, adjustments, panelsOpen, onPanelToggle,
  onLayerClick, onToggleVisibility, onToggleLock, onDeleteLayer, onAddLayer, onDuplicateLayer,
  onBlendChange, onLayerOpacityChange, onAdjChange, onAdjReset,
}: SidebarProps) {
  const activeLayer = layers.find(l => l.id === activeLayerId)
  return (
    <div className="sidebar">
      {/* Histogram */}
      <div className="panel">
        <div className="panel-header" onClick={() => onPanelToggle('hist')}>
          <span className="panel-title">Histogram</span>
          <ChevronRight className={`panel-chevron${panelsOpen.hist ? ' open' : ''}`} size={12} />
        </div>
        {panelsOpen.hist && (
          <div className="histogram-wrap">
            <Histogram />
            <div style={{display:'flex',justifyContent:'space-between',marginTop:4}}>
              {['R','G','B','All'].map(c=><span key={c} style={{fontSize:9.5,color:'var(--text-muted)',fontFamily:'var(--font-mono)'}}>{c}</span>)}
            </div>
          </div>
        )}
      </div>

      <PropertiesPanel
        open={panelsOpen.props}
        onToggle={() => onPanelToggle('props')}
        activeLayer={activeLayer}
        onOpacityChange={v => onLayerOpacityChange(activeLayerId, v)}
      />

      <AdjustmentsPanel
        open={panelsOpen.adj}
        onToggle={() => onPanelToggle('adj')}
        adjustments={adjustments}
        onAdjChange={onAdjChange}
        onReset={onAdjReset}
      />

      <LayersPanel
        open={panelsOpen.layers}
        onToggle={() => onPanelToggle('layers')}
        layers={layers}
        activeLayerId={activeLayerId}
        onLayerClick={onLayerClick}
        onToggleVisibility={onToggleVisibility}
        onToggleLock={onToggleLock}
        onDelete={onDeleteLayer}
        onAdd={onAddLayer}
        onDuplicate={onDuplicateLayer}
        onBlendChange={onBlendChange}
        onOpacityChange={onLayerOpacityChange}
      />
    </div>
  )
}
