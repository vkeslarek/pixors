import { useState, useCallback } from 'react'
import { useEvent, useCommand } from '@/engine/events'

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v))

export function FilterPanel() {
  const [radius, setRadius] = useState(3)
  const dispatch = useCommand('apply_gaussian_blur')
  const [activeTabId, setActiveTabId] = useState<string | null>(null)

  useEvent('tab_state', (ev: any) => setActiveTabId(ev.active_tab_id))
  useEvent('tab_activated', (ev: any) => setActiveTabId(ev.tab_id))
  const [pending, setPending] = useState(false)
  const [lastPct, setLastPct] = useState<number | null>(null)

  useEvent('filter_progress', (e: any) => {
    setLastPct(e.percent)
  })
  useEvent('filter_done', () => {
    setPending(false)
    setLastPct(null)
    console.log('[FilterPanel] gaussian blur done')
  })
  useEvent('filter_failed', (e: any) => {
    setPending(false)
    setLastPct(null)
    console.error('[FilterPanel] gaussian blur failed:', e.error)
  })

  const apply = useCallback(() => {
    if (!activeTabId) return
    setPending(true)
    setLastPct(null)
    console.log(`[FilterPanel] applying gaussian blur radius=${radius} tab=${activeTabId}`)
    dispatch({ tab_id: activeTabId, radius })
  }, [activeTabId, radius, dispatch])

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setRadius(clamp(parseInt(e.target.value) || 1, 1, 32))
  }, [])

  return (
    <div style={{ padding: '8px', display: 'flex', flexDirection: 'column', gap: 8, height: '100%' }}>
      <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)' }}>
        Gaussian Blur
      </div>

      <label style={{ display: 'flex', flexDirection: 'column', gap: 4, fontSize: 11 }}>
        Radius: {radius}
        <input
          type="range"
          min={1}
          max={32}
          value={radius}
          onChange={handleChange}
          disabled={pending}
          style={{ width: '100%' }}
        />
      </label>

      <button
        onClick={apply}
        disabled={pending || !activeTabId}
        style={{
          padding: '4px 8px',
          fontSize: 11,
          cursor: pending ? 'not-allowed' : 'pointer',
          opacity: pending || !activeTabId ? 0.5 : 1,
        }}
      >
        {pending
          ? lastPct != null
            ? `Blurring… ${lastPct}%`
            : 'Blurring…'
          : 'Apply Blur'}
      </button>
    </div>
  )
}
