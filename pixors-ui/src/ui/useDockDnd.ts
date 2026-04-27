import { useCallback } from 'react'
import type { PanelId, DockSide } from './panelLayout'
import { useUIStore, type DropTarget } from './uiStore'

export function useDockDnd() {
  const setDropTarget = useUIStore(s => s.setDropTarget)
  const movePanel = useUIStore(s => s.movePanel)
  const layout = useUIStore(s => s.panelLayout)

  const computeDropTarget = useCallback((e: PointerEvent): DropTarget | null => {
    const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null
    if (!el) return null

    // Check if over existing column
    const colEl = el.closest('[data-col-id]') as HTMLElement | null
    if (colEl) {
      const colId = colEl.dataset.colId!
      const col = layout.columns.find(c => c.id === colId)
      if (!col) return null
      const rect = colEl.getBoundingClientRect()
      const isVertical = col.side === 'left' || col.side === 'right'
      const rel = isVertical
        ? (e.clientX - rect.left) / rect.width
        : (e.clientY - rect.top) / rect.height
      if (rel < 0.25) return { kind: 'before-column', columnId: colId, rect }
      if (rel > 0.75) return { kind: 'after-column', columnId: colId, rect }
      return { kind: 'into-column', columnId: colId, rect }
    }

    // Check if over an empty dock area
    const areaEl = el.closest('[data-dock-zone]') as HTMLElement | null
    if (areaEl) {
      const side = areaEl.dataset.dockZone as DockSide
      const rect = areaEl.getBoundingClientRect()
      return { kind: 'new-column-in-area', side, rect }
    }

    return null
  }, [layout])

  const handleDragMove = useCallback((e: PointerEvent) => {
    setDropTarget(computeDropTarget(e))
  }, [computeDropTarget, setDropTarget])

  const handleDragEnd = useCallback((panelId: PanelId, e: PointerEvent) => {
    const target = computeDropTarget(e)
    setDropTarget(null)
    if (target) movePanel(panelId, target)
  }, [computeDropTarget, setDropTarget, movePanel])

  return { handleDragMove, handleDragEnd }
}
