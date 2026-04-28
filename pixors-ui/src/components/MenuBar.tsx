import { useState } from 'react'
import * as Menubar from '@radix-ui/react-menubar'
import { Check, Minus, Plus, Square, X } from 'lucide-react'
import { useEvent, useCommand } from '@/engine/events'
import { SHORTCUTS } from '@/keymap'
import { useUIStore } from '@/ui/uiStore'
import { DEFAULT_LAYOUT } from '@/ui/panelLayout'

interface LayerInfo {
  id: string; name: string; visible: boolean; opacity: number; blend_mode: string; width: number; height: number; offset_x: number; offset_y: number;
}

interface UITab {
  id: string; name: string; color: string; modified: boolean; hasImage: boolean; width: number; height: number; layerCount?: number; layers?: LayerInfo[];
}

const TAB_COLORS = ['#ff4d4d', '#4dff4d', '#4d4dff', '#ffff4d', '#ff4dff', '#6ffff']

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
  window.dispatchEvent(new CustomEvent('pixors:window', { detail: action }))
  if (action === 'close') { try { window.close() } catch {} }
}

export function MenuBar() {
  const activeTabId = useTabBarState()
  const panelLayout = useUIStore(s => s.panelLayout)
  const togglePanelVisibility = useUIStore(s => s.togglePanelVisibility)
  const setLayout = useUIStore(s => s.setLayout)

  const activeTab = activeTabId ?? null

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
                        if (item.requiresTab && activeTab) item.action(activeTab);
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

// ── TabBar — local state via useEvent ───────────────────

export function TabBar() {
  const [tabs, setTabs] = useState<UITab[]>([])
  const [activeTabId, setActiveTabId] = useState<string | null>(null)

  useEvent('tab_state', (ev) => {
    setTabs(ev.tabs.map((td, i) => ({
      id: td.id, name: td.name, color: TAB_COLORS[i % TAB_COLORS.length], modified: false,
      hasImage: td.has_image, width: td.width, height: td.height,
    })))
    setActiveTabId(ev.active_tab_id)
  })
  useEvent('tab_created', (ev) => {
    setTabs(prev => prev.find(t => t.id === ev.tab_id) ? prev : [...prev, {
      id: ev.tab_id, name: ev.name, color: TAB_COLORS[prev.length % TAB_COLORS.length], modified: false,
      hasImage: false, width: 0, height: 0,
    }])
  })
  useEvent('tab_closed', (ev) => {
    setTabs(prev => prev.filter(t => t.id !== ev.tab_id))
  })
  useEvent('tab_activated', (ev) => setActiveTabId(ev.tab_id))
  useEvent('image_loaded', (ev) => {
    setTabs(prev => prev.map(t => t.id === ev.tab_id ? { ...t, hasImage: true, width: ev.width, height: ev.height } : t))
  })

  const createTab = useCommand('create_tab')
  const closeTab = useCommand('close_tab')
  const activateTab = useCommand('activate_tab')

  return (
    <div className="tabbar">
      {tabs.map(tab => (
        <div
          key={tab.id}
          className={`doc-tab${tab.id === activeTabId ? ' active' : ''}`}
          onClick={() => activateTab({ tab_id: tab.id })}
        >
          <div className="doc-tab-dot" style={{ background: tab.color }} />
          <span>{tab.modified && tab.id === activeTabId ? '● ' : ''}{tab.name}</span>
          <button className="doc-tab-close" onClick={e => { e.stopPropagation(); closeTab({ tab_id: tab.id }) }}>
            <X size={8} />
          </button>
        </div>
      ))}
      <button className="tab-add-btn" onClick={() => createTab()}><Plus size={10} /></button>
    </div>
  )
}

// ── Hook for MenuBar to read activeTabId ───────────────

function useTabBarState(): string | null {
  const [id, setId] = useState<string | null>(null)
  useEvent('tab_state', (ev) => setId(ev.active_tab_id))
  useEvent('tab_activated', (ev) => setId(ev.tab_id))
  return id
}
