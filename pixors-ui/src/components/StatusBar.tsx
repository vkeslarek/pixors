import type { MousePos } from '../types'
import { TOOLS } from './Toolbar'

interface StatusBarProps {
  activeTool: string
  mousePos: MousePos
  zoom: number
  layerCount: number
  connected?: boolean
  error?: string | null
}

export function StatusBar({ activeTool, mousePos, zoom, layerCount, connected = true, error = null }: StatusBarProps) {
  const toolLabel = TOOLS.find(t => t?.id === activeTool)?.label.split(' ')[0] ?? activeTool
  return (
    <div className="statusbar">
      <div className="statusbar-item">
        <span>Tool:</span>
        <span className="statusbar-accent">{toolLabel}</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Canvas: 900×600px</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>X: <span className="statusbar-accent">{mousePos.x}</span></span>
        &nbsp;
        <span>Y: <span className="statusbar-accent">{mousePos.y}</span></span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Zoom: <span className="statusbar-accent">{zoom}%</span></span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item">
        <span>Layers: <span className="statusbar-accent">{layerCount}</span></span>
      </div>
      <div style={{ flex: 1 }} />
      <div className="statusbar-item" title={error ? `Error: ${error}` : connected ? 'Engine connected' : 'Engine disconnected'}>
        <span style={{
          display: 'inline-block',
          width: 8,
          height: 8,
          borderRadius: '50%',
          backgroundColor: error ? 'var(--error)' : connected ? 'oklch(0.72 0.15 145)' : 'var(--text-muted)',
          marginRight: 6,
          verticalAlign: 'middle',
        }} />
        <span style={{ fontSize: 11 }}>{error ? 'Error' : connected ? 'Connected' : 'Disconnected'}</span>
      </div>
      <div className="statusbar-sep" />
      <div className="statusbar-item"><span>RGB/8</span></div>
      <div className="statusbar-sep" />
      <div className="statusbar-item"><span>sRGB</span></div>
    </div>
  )
}
