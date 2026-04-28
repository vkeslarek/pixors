import { useState } from 'react'
import { useEvent, useConnected } from '@/engine/events'
import { useUIStore } from '@/ui/uiStore'
import { TOOLS } from '@/components/Toolbar'

export function StatusBar() {
  const [activeTool, setActiveTool] = useState('pan')
  const [zoom, setZoom] = useState(1)
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [layerCount, setLayerCount] = useState(0)
  const [imageSize, setImageSize] = useState<{ w: number; h: number } | null>(null)
  const [lastError, setLastError] = useState<string | null>(null)

  const connected = useConnected()
  const mousePos = useUIStore(s => s.mousePos)

  useEvent('tool_state', (ev) => setActiveTool(ev.tool))
  useEvent('tool_changed', (ev) => setActiveTool(ev.tool))
  useEvent('tab_state', (ev) => setActiveTabId(ev.active_tab_id))
  useEvent('tab_activated', (ev) => setActiveTabId(ev.tab_id))
  useEvent('viewport_state', (ev) => { if (ev.tab_id === activeTabId) setZoom(ev.zoom) })
  useEvent('viewport_updated', (ev) => { if (ev.tab_id === activeTabId) setZoom(ev.zoom) })
  useEvent('layer_state', (ev) => { if (ev.tab_id === activeTabId) setLayerCount(ev.layers.length) })
  useEvent('image_loaded', (ev) => { if (ev.tab_id === activeTabId) { setLayerCount(ev.layer_count); setImageSize({ w: ev.width, h: ev.height }) } })
  useEvent('doc_size_changed', (ev) => { if (ev.tab_id === activeTabId) setImageSize({ w: ev.width, h: ev.height }) })
  useEvent('error', (ev) => setLastError(ev.message))

  const toolLabel = TOOLS.find(t => t?.id === activeTool)?.label.split(' ')[0] ?? activeTool
  return (
    <div className="statusbar">
      <div className="statusbar-item">
        <span>Tool:</span>
        <span className="statusbar-accent">{toolLabel}</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Canvas: {imageSize ? `${imageSize.w}×${imageSize.h}px` : '—'}</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>X: <span className="statusbar-accent">{mousePos.x}</span></span>
        &nbsp;
        <span>Y: <span className="statusbar-accent">{mousePos.y}</span></span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Zoom: <span className="statusbar-accent">{(zoom * 100).toFixed(0)}%</span></span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Layers: <span className="statusbar-accent">{layerCount}</span></span>
      </div>
      <div style={{ flex: 1 }} />
      <div className="statusbar-item" title={lastError ? `Error: ${lastError}` : connected ? 'Engine connected' : 'Engine disconnected'}>
        <span style={{
          display: 'inline-block', width: 8, height: 8, borderRadius: '50%',
          backgroundColor: lastError ? 'var(--error)' : connected ? 'oklch(0.72 0.15 145)' : 'var(--text-muted)',
          marginRight: 6, verticalAlign: 'middle',
        }} />
        <span style={{ fontSize: 11 }}>{lastError ? 'Error' : connected ? 'Connected' : 'Disconnected'}</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item"><span>RGB/8</span></div>
      <div className="statusbar-sep" />
      <div className="statusbar-item"><span>sRGB</span></div>
    </div>
  )
}
