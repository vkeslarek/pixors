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
  { id: 'library',  icon: Images,    label: 'Library — Browse & Organize',  available: false },
  { id: 'darkroom', icon: SunMedium, label: 'Darkroom — Develop & Adjust',  available: false },
  { id: 'editor',   icon: Layers,    label: 'Editor — Composite & Retouch', available: true  },
]

export function WorkspaceBar() {
  const workspace = useUIStore(s => s.workspace)
  const setWorkspace = useUIStore(s => s.setWorkspace)

  return (
    <div className="workspace-bar">
      <div className="workspace-top">
        {WORKSPACES.map(ws => (
          <Tooltip.Root key={ws.id} delayDuration={300}>
            <Tooltip.Trigger asChild>
              <button
                className={`workspace-btn${!ws.available ? ' disabled' : ''}${workspace === ws.id ? ' active' : ''}`}
                onClick={() => ws.available && setWorkspace(ws.id)}
              >
                <ws.icon size={20} />
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
      <div className="workspace-bottom">
        <button className="workspace-btn"><Settings size={16} /></button>
        <button className="workspace-btn"><HelpCircle size={16} /></button>
      </div>
    </div>
  )
}
