import * as Menubar from '@radix-ui/react-menubar'
import { Check, Minus, Plus, Square, X } from 'lucide-react'
import { useTabs, useActiveTabId, useActiveTab, engine } from '@/engine'
import { SHORTCUTS } from '@/keymap'
import { useUIStore } from '@/ui/uiStore'
import { DEFAULT_LAYOUT } from '@/ui/panelLayout'

const MENU_ITEMS = [
  {
    label: 'File',
    items: [
      SHORTCUTS.openFile,
      SHORTCUTS.closeTab,
    ],
  },
  {
    label: 'View',
    items: [
      SHORTCUTS.zoomIn,
      SHORTCUTS.zoomOut,
      SHORTCUTS.fitToScreen,
      SHORTCUTS.actualSize,
    ],
  },
]

function capitalize(s: string) { return s.charAt(0).toUpperCase() + s.slice(1) }

type WindowAction = 'minimize' | 'maximize' | 'close'
function windowAction(action: WindowAction) {
  // Wry/Tauri: dispatch custom event for Rust shell to handle
  window.dispatchEvent(new CustomEvent('pixors:window', { detail: action }))
  // Browser fallback: close only
  if (action === 'close') {
    try { window.close() } catch {}
  }
}

export function MenuBar() {
  const activeTab = useActiveTab()
  const panelLayout = useUIStore(s => s.panelLayout)
  const togglePanelVisibility = useUIStore(s => s.togglePanelVisibility)
  const setLayout = useUIStore(s => s.setLayout)

  return (
    <div className="menubar" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}>
      <div className="menubar-logo">PIXORS</div>
      <Menubar.Root className="menubar-root" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        {MENU_ITEMS.map(menu => (
          <Menubar.Menu key={menu.label}>
            <Menubar.Trigger className="menu-item">{menu.label}</Menubar.Trigger>
            <Menubar.Portal>
              <Menubar.Content className="dropdown-content" sideOffset={4}>
                {menu.items.map(item => {
                  const disabled = item.requiresTab && !activeTab;
                  return (
                    <Menubar.Item
                      key={item.label}
                      className="dropdown-item"
                      disabled={disabled}
                      onSelect={() => {
                        if (disabled) return;
                        if (item.requiresTab && activeTab) item.action(activeTab.id);
                        else item.action('' as any);
                      }}
                    >
                      <span>{item.label}</span>
                      {item.shortcut && <span className="menu-shortcut">{item.shortcut}</span>}
                    </Menubar.Item>
                  )
                })}
              </Menubar.Content>
            </Menubar.Portal>
          </Menubar.Menu>
        ))}

        <Menubar.Menu>
          <Menubar.Trigger className="menu-item">Window</Menubar.Trigger>
          <Menubar.Portal>
            <Menubar.Content className="dropdown-content" sideOffset={4}>
              <Menubar.Sub>
                <Menubar.SubTrigger className="dropdown-item">
                  Panels <span style={{ marginLeft: 'auto', paddingLeft: 16 }}>▶</span>
                </Menubar.SubTrigger>
                <Menubar.Portal>
                  <Menubar.SubContent className="dropdown-content" sideOffset={2} alignOffset={-4}>
                    {Object.values(panelLayout?.panels || {}).map(p => (
                      <Menubar.CheckboxItem
                        key={p.id}
                        className="dropdown-item"
                        checked={p.columnId !== null}
                        onCheckedChange={() => togglePanelVisibility(p.id)}
                      >
                        <Menubar.ItemIndicator className="menu-indicator">
                          <Check size={14} />
                        </Menubar.ItemIndicator>
                        <span>{capitalize(p.id)}</span>
                      </Menubar.CheckboxItem>
                    ))}
                  </Menubar.SubContent>
                </Menubar.Portal>
              </Menubar.Sub>
              <Menubar.Separator className="dropdown-separator" />
              <Menubar.Item className="dropdown-item" onSelect={() => setLayout(DEFAULT_LAYOUT)}>
                <span>Reset Layout</span>
              </Menubar.Item>
            </Menubar.Content>
          </Menubar.Portal>
        </Menubar.Menu>
      </Menubar.Root>

      {/* Window controls (drag region, but buttons are no-drag) */}
      <div className="window-controls" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        <button className="window-btn" title="Minimize" onClick={() => windowAction('minimize')}>
          <Minus size={14} />
        </button>
        <button className="window-btn" title="Maximize" onClick={() => windowAction('maximize')}>
          <Square size={12} />
        </button>
        <button className="window-btn window-btn-close" title="Close" onClick={() => windowAction('close')}>
          <X size={14} />
        </button>
      </div>
    </div>
  )
}

export function TabBar() {
  const tabs = useTabs()
  const activeTabId = useActiveTabId()

  return (
    <div className="tabbar">
      {tabs.map(tab => (
        <div
          key={tab.id}
          className={`doc-tab${tab.id === activeTabId ? ' active' : ''}`}
          onClick={() => engine.dispatch({ type: 'activate_tab', tab_id: tab.id })}
        >
          <div className="doc-tab-dot" style={{ background: tab.color }} />
          <span>{tab.modified && tab.id === activeTabId ? '● ' : ''}{tab.name}</span>
          <button className="doc-tab-close" onClick={e => { e.stopPropagation(); engine.dispatch({ type: 'close_tab', tab_id: tab.id }) }}>
            <X size={8} />
          </button>
        </div>
      ))}
      <button className="tab-add-btn" onClick={() => engine.dispatch({ type: 'create_tab' })}><Plus size={10} /></button>
    </div>
  )
}
