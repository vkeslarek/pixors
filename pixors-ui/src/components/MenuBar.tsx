import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { FolderOpen, Download, Plus, X } from 'lucide-react'
import type { UITab as Tab } from '../engine/types'

const MENU_ITEMS = [
  { label: 'File', items: ['New', 'Open...', 'Save', 'Save As...', 'Export...', 'Close'] },
  { label: 'Edit', items: ['Undo', 'Redo', 'Cut', 'Copy', 'Paste', 'Preferences'] },
  { label: 'Image', items: ['Adjustments', 'Transform', 'Canvas Size', 'Image Size', 'Crop'] },
  { label: 'Layer', items: ['New Layer', 'Duplicate Layer', 'Delete Layer', 'Merge Layers'] },
  { label: 'Select', items: ['All', 'None', 'Inverse', 'Feather...'] },
  { label: 'Filter', items: ['Blur', 'Sharpen', 'Noise', 'Distort', 'Render'] },
  { label: 'View', items: ['Zoom In', 'Zoom Out', 'Fit to Screen', 'Actual Size', 'Rulers'] },
  { label: 'Window', items: ['Layers', 'Properties', 'History', 'Color', 'Tools'] },
  { label: 'Help', items: ['Documentation', 'About'] },
]

interface MenuBarProps {
  activeTabName?: string
  onOpenFile: () => void
  onExport: () => void
}

export function MenuBar({ activeTabName, onOpenFile, onExport }: MenuBarProps) {
  return (
    <div className="menubar">
      <div className="menubar-logo">PIXORS</div>
      {MENU_ITEMS.map(menu => (
        <DropdownMenu.Root key={menu.label}>
          <DropdownMenu.Trigger asChild>
            <button className="menu-item">{menu.label}</button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content className="dropdown-content" sideOffset={4}>
              {menu.items.map(item => (
                <DropdownMenu.Item
                  key={item}
                  className="dropdown-item"
                  onSelect={() => {
                    if (menu.label === 'File' && item === 'Open...') onOpenFile()
                    if (menu.label === 'File' && item === 'Export...') onExport()
                  }}
                >
                  {item}
                </DropdownMenu.Item>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      ))}
      <div className="menubar-right">
        {activeTabName && <span className="document-name">{activeTabName}</span>}
        <button className="btn btn-outline" onClick={onOpenFile}><FolderOpen size={13} /> Open</button>
        <button className="btn btn-accent" onClick={onExport}><Download size={13} /> Export</button>
      </div>
    </div>
  )
}

// ── TabBar — lives inside the canvas column, not full-width ──────────────────
interface TabBarProps {
  tabs: Tab[]
  activeTabId: string | null
  onTabClick: (id: string) => void
  onTabClose: (id: string) => void
  onTabAdd: () => void
}

export function TabBar({ tabs, activeTabId, onTabClick, onTabClose, onTabAdd }: TabBarProps) {
  return (
    <div className="tabbar">
      {tabs.map(tab => (
        <div
          key={tab.id}
          className={`doc-tab${tab.id === activeTabId ? ' active' : ''}`}
          onClick={() => onTabClick(tab.id)}
        >
          <div className="doc-tab-dot" style={{ background: tab.color }} />
          <span>{tab.modified && tab.id === activeTabId ? '● ' : ''}{tab.name}</span>
          <button className="doc-tab-close" onClick={e => { e.stopPropagation(); onTabClose(tab.id) }}>
            <X size={8} />
          </button>
        </div>
      ))}
      <button className="tab-add-btn" onClick={onTabAdd}><Plus size={10} /></button>
    </div>
  )
}
