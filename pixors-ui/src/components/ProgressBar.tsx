import { useState } from 'react'
import { useEvent } from '@/engine/events'

export function ProgressBar() {
  const [activeTabId, setActiveTabId] = useState<string | null>(null)
  const [percent, setPercent] = useState(0)

  useEvent('tab_state', (ev) => setActiveTabId(ev.active_tab_id))
  useEvent('tab_activated', (ev) => setActiveTabId(ev.tab_id))
  useEvent('image_load_progress', (ev) => setPercent(ev.percent))
  useEvent('image_loaded', () => { if (activeTabId) setPercent(100) })
  useEvent('image_closed', () => setPercent(0))

  return (
    <div className="progressbar">
      <div className="progressbar-fill" style={{ width: `${percent}%` }} />
    </div>
  )
}
