import { useMemo, useRef, useState } from 'react'
import type { DockColumn as DockColumnType, PanelId } from '@/ui/panelLayout'
import { useUIStore } from '@/ui/uiStore'
import { PanelFrame } from './PanelFrame'

interface DockColumnProps {
  column: DockColumnType
  isLast: boolean
}

export function DockColumn({ column }: DockColumnProps) {
  const layout = useUIStore(s => s.panelLayout)
  const resizeColumn = useUIStore(s => s.resizeColumn)
  const colRef = useRef<HTMLDivElement>(null)

  const panelsInColumn = useMemo(() => {
    return Object.values(layout.panels)
      .filter(p => p.columnId === column.id)
      .sort((a, b) => a.order - b.order) as Array<{ id: PanelId; columnId: string; order: number }>
  }, [layout, column.id])

  const isVertical = column.side === 'left' || column.side === 'right'
  const isRightSide = column.side === 'right'
  const isBottomSide = column.side === 'bottom'

  const effectiveSize = column.size
  const [renderedWidth, setRenderedWidth] = useState(effectiveSize)
  const showResize = true

  // ── Resize with DOM manipulation during drag (no flicker) ────────
  const dragState = useRef<{ start: number; initialSize: number }>({ start: 0, initialSize: 0 })

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault(); e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    dragState.current = {
      start: isVertical ? e.clientX : e.clientY,
      initialSize: effectiveSize,
    }
  }

  const onPointerMove = (e: React.PointerEvent) => {
    if (dragState.current.start === 0) return
    const current = isVertical ? e.clientX : e.clientY
    let delta = current - dragState.current.start
    if (isRightSide || isBottomSide) delta = -delta
    const newSize = Math.max(78, dragState.current.initialSize + delta)
    setRenderedWidth(newSize)
    if (colRef.current && isVertical) {
      colRef.current.style.width = `${newSize}px`
    }
  }

  const onPointerUp = () => {
    if (dragState.current.start === 0) return
    const current = isVertical
      ? colRef.current?.style.width?.replace('px', '') || column.size.toString()
      : effectiveSize.toString()
    resizeColumn(column.id, parseInt(current))
    dragState.current = { start: 0, initialSize: 0 }
  }

  const resizeAxis: 'x' | 'y' = isVertical ? 'x' : 'y'
  const handleStyle: React.CSSProperties = isVertical
    ? { position: 'absolute', top: 0, bottom: 0, [isRightSide ? 'left' : 'right']: -2, width: 4, cursor: 'col-resize', zIndex: 5 }
    : { position: 'absolute', left: 0, right: 0, [isBottomSide ? 'top' : 'bottom']: -2, height: 4, cursor: 'row-resize', zIndex: 5 }

  return (
    <div ref={colRef}
      data-col-id={column.id}
      style={{
        display: 'flex', flexDirection: isVertical ? 'column' : 'row',
        width: isVertical ? effectiveSize : 'auto',
        height: !isVertical ? effectiveSize : 'auto',
        flexShrink: 0, overflow: 'hidden', position: 'relative',
      }}
    >
      {panelsInColumn.map((panel) => (
        <PanelFrame key={panel.id} panelId={panel.id} showTitle={renderedWidth > 80} />
      ))}
      {showResize && (
        <div className={`resize-handle resize-${resizeAxis}`} style={handleStyle}
          onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} />
      )}
    </div>
  )
}
