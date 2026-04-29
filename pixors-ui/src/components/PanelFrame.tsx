import { GripVertical, X } from 'lucide-react'
import { useState, useCallback, useRef, useEffect } from 'react'
import type { PanelId } from '@/ui/panelLayout'
import { useUIStore } from '@/ui/uiStore'
import { useDockDnd } from '@/ui/useDockDnd'
import { LayersPanel } from './panels/LayersPanel'
import { FilterPanel } from './panels/FilterPanel'
import { Toolbar } from './Toolbar'

interface PanelFrameProps {
  panelId: PanelId
  showTitle?: boolean
}

export function PanelFrame({ panelId, showTitle = true }: PanelFrameProps) {
  const toggleVisibility = useUIStore(s => s.togglePanelVisibility)
  const setDraggingPanel = useUIStore(s => s.setDraggingPanel)
  const dragging = useUIStore(s => s.draggingPanel === panelId)
  const { handleDragMove, handleDragEnd: dndDragEnd } = useDockDnd()

  const [animating, setAnimating] = useState(false)
  const layout = useUIStore(s => s.panelLayout)
  const currentColId = layout.panels[panelId]?.columnId

  useEffect(() => {
    setAnimating(true)
    const t = setTimeout(() => setAnimating(false), 250)
    return () => clearTimeout(t)
  }, [currentColId])

  const dragRef = useRef<{ startX: number; startY: number; active: boolean }>({ startX: 0, startY: 0, active: false })

  const onDragStart = useCallback((e: React.PointerEvent) => {
    if ((e.target as HTMLElement).closest('button')) return
    e.preventDefault()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    dragRef.current = { startX: e.clientX, startY: e.clientY, active: false }

    const handleWindowMove = (evt: PointerEvent) => {
      const dx = Math.abs(evt.clientX - dragRef.current.startX)
      const dy = Math.abs(evt.clientY - dragRef.current.startY)
      if (dx > 5 || dy > 5) {
        if (!dragRef.current.active) {
          dragRef.current.active = true
          setDraggingPanel(panelId)
        }
        handleDragMove(evt)
      }
    }

    const handleWindowUp = (evt: PointerEvent) => {
      const wasActive = dragRef.current.active
      dragRef.current = { startX: 0, startY: 0, active: false }
      setDraggingPanel(null)
      if (wasActive) dndDragEnd(panelId, evt)
      window.removeEventListener('pointermove', handleWindowMove)
      window.removeEventListener('pointerup', handleWindowUp)
    }

    window.addEventListener('pointermove', handleWindowMove)
    window.addEventListener('pointerup', handleWindowUp)
  }, [panelId, setDraggingPanel, handleDragMove, dndDragEnd])

  return (
    <div
      className={`panel expandable${dragging ? ' dragging' : ''}${animating ? ' animating' : ''}`}
      style={{ flex: '1 1 0', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}
    >
      <div
        className="panel-header"
        style={{ display: 'flex', alignItems: 'center', cursor: 'grab', flexShrink: 0, padding: '0 4px', gap: 4, minWidth: 0 }}
        onPointerDown={onDragStart}
      >
        <GripVertical size={12} style={{ flexShrink: 0, color: 'var(--text-muted)' }} />
        {showTitle && (
          <span className="panel-title" style={{ flex: '1 1 0', minWidth: 0, overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' }}>
            {panelId.charAt(0).toUpperCase() + panelId.slice(1)}
          </span>
        )}
        {!showTitle && <span style={{ flex: 1 }} />}
        <button className="icon-btn" title="Hide" onClick={() => toggleVisibility(panelId)} style={{ flexShrink: 0 }}>
          <X size={12} />
        </button>
      </div>

      {panelId === 'layers' && <LayersPanel />}
      {panelId === 'filters' && <FilterPanel />}
      {panelId === 'toolbar' && <Toolbar />}
    </div>
  )
}
