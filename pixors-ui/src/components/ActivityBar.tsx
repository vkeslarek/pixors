import * as Tooltip from '@radix-ui/react-tooltip'
import { Images, SunMedium, Layers, Settings, HelpCircle } from 'lucide-react'

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

interface ActivityBarProps {
  workspace: Workspace
  onWorkspaceChange: (w: Workspace) => void
}

export function ActivityBar({ workspace, onWorkspaceChange }: ActivityBarProps) {
  return (
    <div className="activity-bar">
      {/* Workspace switchers */}
      <div className="activity-top">
        {WORKSPACES.map(ws => (
          <Tooltip.Root key={ws.id} delayDuration={300}>
            <Tooltip.Trigger asChild>
              <button
                className={[
                  'activity-btn',
                  workspace === ws.id ? 'active' : '',
                  !ws.available ? 'disabled' : '',
                ].filter(Boolean).join(' ')}
                onClick={() => ws.available && onWorkspaceChange(ws.id)}
                aria-label={ws.label}
              >
                <ws.icon size={20} />
                {workspace === ws.id && <div className="activity-indicator" />}
              </button>
            </Tooltip.Trigger>
            <Tooltip.Portal>
              <Tooltip.Content className="tooltip-content" side="right" sideOffset={8}>
                {ws.label}
                {!ws.available && (
                  <span className="activity-soon"> · Coming soon</span>
                )}
                <Tooltip.Arrow className="tooltip-arrow" />
              </Tooltip.Content>
            </Tooltip.Portal>
          </Tooltip.Root>
        ))}
      </div>

      {/* Bottom actions */}
      <div className="activity-bottom">
        <Tooltip.Root delayDuration={300}>
          <Tooltip.Trigger asChild>
            <button className="activity-btn" aria-label="Settings">
              <Settings size={18} />
            </button>
          </Tooltip.Trigger>
          <Tooltip.Portal>
            <Tooltip.Content className="tooltip-content" side="right" sideOffset={8}>
              Settings
              <Tooltip.Arrow className="tooltip-arrow" />
            </Tooltip.Content>
          </Tooltip.Portal>
        </Tooltip.Root>

        <Tooltip.Root delayDuration={300}>
          <Tooltip.Trigger asChild>
            <button className="activity-btn" aria-label="Help">
              <HelpCircle size={18} />
            </button>
          </Tooltip.Trigger>
          <Tooltip.Portal>
            <Tooltip.Content className="tooltip-content" side="right" sideOffset={8}>
              Help & Documentation
              <Tooltip.Arrow className="tooltip-arrow" />
            </Tooltip.Content>
          </Tooltip.Portal>
        </Tooltip.Root>
      </div>
    </div>
  )
}
