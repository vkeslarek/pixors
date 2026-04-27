import * as Tooltip from '@radix-ui/react-tooltip'
import { Move, Square, Circle, Wand, Crop, Droplet, Brush, Eraser, Heart, Palette, FileText, Shapes, Hand, ZoomIn } from 'lucide-react'
import { useTool, engine } from '@/engine'

export const TOOLS = [
  { id: 'move', icon: Move, label: 'Move (V)' },
  { id: 'select', icon: Square, label: 'Marquee Select (M)' },
  { id: 'lasso', icon: Circle, label: 'Lasso Select (L)' },
  { id: 'wand', icon: Wand, label: 'Magic Wand (W)' },
  null,
  { id: 'crop', icon: Crop, label: 'Crop (C)' },
  { id: 'eyedropper', icon: Droplet, label: 'Eyedropper (I)' },
  null,
  { id: 'brush', icon: Brush, label: 'Brush (B)' },
  { id: 'eraser', icon: Eraser, label: 'Eraser (E)' },
  { id: 'heal', icon: Heart, label: 'Healing (J)' },
  { id: 'gradient', icon: Palette, label: 'Gradient (G)' },
  null,
  { id: 'text', icon: FileText, label: 'Text (T)' },
  { id: 'shape', icon: Shapes, label: 'Shape (U)' },
  null,
  { id: 'hand', icon: Hand, label: 'Hand (H)' },
  { id: 'zoom', icon: ZoomIn, label: 'Zoom (Z)' },
] as const

export function Toolbar() {
  const activeTool = useTool()
  return (
    <div className="toolbar">
      {TOOLS.map((tool, i) =>
        tool === null ? (
          <div key={`sep-${i}`} className="tool-sep" />
        ) : (
          <Tooltip.Root key={tool.id} delayDuration={400}>
            <Tooltip.Trigger asChild>
              <button
                className={`tool-btn${activeTool === tool.id ? ' active' : ''}`}
                onClick={() => engine.dispatch({ type: 'select_tool', tool: tool.id })}
              >
                <tool.icon size={20} />
              </button>
            </Tooltip.Trigger>
            <Tooltip.Portal>
              <Tooltip.Content className="tooltip-content" side="right" sideOffset={6}>
                {tool.label}
                <Tooltip.Arrow className="tooltip-arrow" />
              </Tooltip.Content>
            </Tooltip.Portal>
          </Tooltip.Root>
        )
      )}
    </div>
  )
}
