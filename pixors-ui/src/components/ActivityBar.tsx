import * as Tooltip from '@radix-ui/react-tooltip'
import { Images, SunMedium, Layers, Settings, HelpCircle } from 'lucide-react'
import { useUIStore } from '@/ui/uiStore'

export type Workspace = 'library' | 'darkroom' | 'editor'

interface WorkspaceItem {
  id: Workspace
  icon: React.FC<{ size?: number }>
  label: string
  available: boolean
}

const WORKSPACES: WorkspaceItem[] = [
  { id: 'library',  icon: Images,      label: 'Library — Browse & Organize',  available: false },
  { id: 'darkroom', icon: SunMedium,   label: 'Darkroom — Develop & Adjust',  available: false },
  { id: 'editor',   icon: Layers,      label: 'Editor — Composite & Retouch', available: true  },
]

export function ActivityBar() {
  const workspace = useUIStore(s => s.workspace)
  const setWorkspace = useUIStore(s => s.setWorkspace)

  return (
    <div className="activity-bar">
      {/* Workspace switchers */}
      <div className="activity-top">
        {WORKSPACES.map(ws => (
          <Tooltip.Root key={ws.id} delayDuration={300}>
            <Tooltip.Trigger asChild>
              <button
                className={`activity-btn${!ws.available ? ' disabled' : ''}${workspace === ws.id ? ' active' : ''}`}
                onClick={() => ws.available && setWorkspace(ws.id)}
              >
                <ws.icon size={14} />
              </button>
            </Tooltip.Trigger>
            <Tooltip.Portal>
              <Tooltip.Content className="tooltip-content" side="right" sideOffset={6}>
                {ws.label}
                <Tooltip.Arrow className="tooltip-arrow" />
              </Tooltip.Content>
            </Tooltip.Portal>
          </Tooltip.Root>
        ))}
      </div>
      <div className="activity-bottom">
        <button className="activity-btn"><Settings size={12} /></button>
        <button className="activity-btn"><HelpCircle size={12} /></button>
      </div>
    </div>
  )
}
