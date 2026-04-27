import { Eye, EyeOff, Lock, Trash2, Plus, Copy, Filter } from 'lucide-react'
import { useActiveTab } from '@/engine'

const BLEND_MODES = ['Normal','Dissolve','Multiply','Screen','Overlay','Soft Light','Hard Light','Color Dodge','Color Burn','Darken','Lighten','Difference','Exclusion','Hue','Saturation','Color','Luminosity']

export function LayersPanel() {
  const activeTab = useActiveTab()
  const layers = (activeTab as any)?.layers || []

  if (layers.length === 0) {
    return (
      <div style={{ padding: 16, color: 'var(--text-muted)', fontSize: 12, textAlign: 'center' }}>
        No layers yet.
      </div>
    )
  }

  return (
    <>
      <div className="layers-toolbar">
        <select className="blend-select" value="Normal" disabled>
          {BLEND_MODES.map(m => <option key={m}>{m}</option>)}
        </select>
        <input className="opacity-input" type="number" min={0} max={100} defaultValue={100} disabled />
        <span className="pct">%</span>
      </div>
      <div className="panel-body" style={{ flex: 1 }}>
        {layers.map((layer: any) => (
          <div key={layer.id || layer.name}
            className="layer-item"
            style={{ opacity: layer.visible !== false ? 1 : 0.4 }}>
            <div className="layer-thumb">
              <div className="layer-thumb-checker" />
              <div className="layer-thumb-color" style={{ background: layer.color || '#ccc', opacity: .7 }} />
            </div>
            <div className="layer-info">
              <div className="layer-name">{layer.name || `Layer ${layer.id}`}</div>
              <div className="layer-type">{layer.type || 'RGBA'} · {layer.blendMode || 'Normal'}</div>
            </div>
            <div className="layer-actions">
              <button className="icon-btn" title="Visibility (read-only)" disabled>
                {layer.visible !== false ? <Eye size={12} /> : <EyeOff size={12} />}
              </button>
              <button className="icon-btn" title="Lock (read-only)" disabled>
                <Lock size={12} />
              </button>
            </div>
          </div>
        ))}
      </div>
      <div className="layers-bottom">
        <button className="icon-btn" disabled title="Add layer (read-only)"><Plus size={14} /></button>
        <button className="icon-btn" disabled title="Duplicate (read-only)"><Copy size={14} /></button>
        <button className="icon-btn" disabled title="Layer effects (read-only)"><Filter size={14} /></button>
        <div style={{ flex: 1 }} />
        <button className="icon-btn" disabled title="Delete (read-only)" style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
      </div>
    </>
  )
}
