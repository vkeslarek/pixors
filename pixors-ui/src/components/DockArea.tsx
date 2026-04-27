import type { DockSide } from '@/ui/panelLayout'
import { useUIStore } from '@/ui/uiStore'
import { DockColumn } from './DockColumn'

interface DockAreaProps {
  side: DockSide
}

export function DockArea({ side }: DockAreaProps) {
  const layout = useUIStore(s => s.panelLayout)
  const isDragging = useUIStore(s => s.draggingPanel !== null)

  const columns = layout.columns.filter(c => c.side === side)
  const isVertical = side === 'left' || side === 'right'

  if (columns.length === 0) {
    if (!isDragging) return null
    // Phantom drop zone: visible when dragging, gives user a drop target on empty sides
    return (
      <div
        data-dock-zone={side}
        className="dock-area-empty"
        style={{
          width: isVertical ? 40 : 'auto',
          height: !isVertical ? 40 : '100%',
          flexShrink: 0,
        }}
      />
    )
  }

  return (
    <div
      data-dock-zone={side}
      className="dock-area"
      style={{
        display: 'flex',
        flexDirection: isVertical ? 'row' : 'column',
        alignItems: 'stretch',
        flexShrink: 0,
      }}
    >
      {columns.map((col, idx) => (
        <DockColumn key={col.id} column={col} isLast={idx === columns.length - 1} />
      ))}
    </div>
  )
}
