import { useState, useEffect } from 'react'
import type { CSSProperties } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import * as Toast from '@radix-ui/react-toast'
import { MenuBar, TabBar } from '@/components/MenuBar'
import { WorkspaceBar } from '@/components/WorkspaceBar'
import { Viewport } from '@/components/Viewport'
import { DockArea } from '@/components/DockArea'
import { StatusBar } from '@/components/StatusBar'
import { ProgressBar } from '@/components/ProgressBar'
import '@/App.css'

import { registerShortcuts } from '@/keymap'
import { useEvent } from '@/engine/events'
import { useUIStore } from '@/ui/uiStore'

function useKeymap() {
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  useEvent('tab_state', (ev) => setActiveTabId(ev.active_tab_id))
  useEvent('tab_activated', (ev) => setActiveTabId(ev.tab_id))
  useEffect(() => { return registerShortcuts(activeTabId) }, [activeTabId])
}

function GlobalToaster() {
  const [lastError, setLastError] = useState<string | null>(null)
  const [open, setOpen] = useState(false)

  useEvent('error', (ev) => {
    setLastError(ev.message)
    setOpen(true)
  })

  return (
    <Toast.Provider swipeDirection="right">
      <Toast.Root className="toast-root" open={open} onOpenChange={setOpen}>
        <Toast.Title className="toast-title">Error</Toast.Title>
        <Toast.Description className="toast-description">{lastError}</Toast.Description>
        <Toast.Action className="toast-action" asChild altText="Close">
          <button className="btn btn-outline" onClick={() => setOpen(false)}>Close</button>
        </Toast.Action>
      </Toast.Root>
      <Toast.Viewport className="toast-viewport" />
    </Toast.Provider>
  )
}

function DropPreviewOverlay() {
  const dropTarget = useUIStore(s => s.dropTarget)
  if (!dropTarget) return null

  const { rect, kind } = dropTarget
  const style: CSSProperties = {
    position: 'fixed',
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
    pointerEvents: 'none',
    zIndex: 9999,
  }

  return <div className={`drop-preview drop-preview-${kind}`} style={style} />
}

export default function App() {
  useKeymap()

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar />
        <div className="workspace" style={{ display: 'flex', position: 'relative', overflow: 'hidden', minWidth: 0 }}>
          <WorkspaceBar />
          <DockArea side="left" />
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, overflow: 'hidden' }}>
            <TabBar />
            <Viewport />
            <DockArea side="bottom" />
          </div>
          <DockArea side="right" />
        </div>
        <DropPreviewOverlay />
        <ProgressBar />
        <StatusBar />
        <GlobalToaster />
      </div>
    </Tooltip.Provider>
  )
}
