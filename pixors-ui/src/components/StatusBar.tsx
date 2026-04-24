import { useState, useEffect } from 'react'
import type { MousePos } from '../types'
import { TOOLS } from './Toolbar'

interface StatusBarProps {
  activeTool: string
  zoom: number
  layerCount: number
  connected?: boolean
  error?: string | null
}

/**
 * StatusBar Component
 * Displays current tool, canvas info, mouse position, and engine connection status.
 *
 * Performance note:
 * To avoid triggering full-app React renders during high-frequency mouse movements (e.g. 60+ Hz panning),
 * the `mousePos` state is fully internalized. It listens directly to the native DOM `mouse_pos` 
 * CustomEvent dispatched by the Viewport, completely bypassing the parent component tree.
 */
export function StatusBar({ activeTool, zoom, layerCount, connected = true, error = null }: StatusBarProps) {
  const [mousePos, setMousePos] = useState<MousePos>({ x: 0, y: 0 });

  useEffect(() => {
    // Native event listener for decoupled high-performance updates
    const onMousePos = (e: Event) => {
      const customEvent = e as CustomEvent;
      setMousePos(customEvent.detail);
    };
    window.addEventListener('mouse_pos', onMousePos);
    return () => window.removeEventListener('mouse_pos', onMousePos);
  }, []);

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
