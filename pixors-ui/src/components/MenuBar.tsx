import * as DropdownMenu from '@radix-ui/react-dropdown-menu'
import { FolderOpen, Download, Plus, X } from 'lucide-react'
import { useTabs, useActiveTabId, useActiveTab, engine } from '@/engine'

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

export function MenuBar() {
  const activeTab = useActiveTab()

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
                    if (menu.label === 'File' && item === 'Open...') engine.dispatch({ type: 'open_file_dialog' })
                    if (menu.label === 'File' && item === 'Export...') console.log('export')
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
        {activeTab?.name && <span className="document-name">{activeTab.name}</span>}
        <button className="btn btn-outline" onClick={() => engine.dispatch({ type: 'open_file_dialog' })}><FolderOpen size={13} /> Open</button>
        <button className="btn btn-accent" onClick={() => console.log('export')}><Download size={13} /> Export</button>
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
