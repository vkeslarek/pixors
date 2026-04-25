import { useActiveTabId, useLoadingFor } from '@/engine'

export function ProgressBar() {
  const activeTabId = useActiveTabId()
  const { percent } = useLoadingFor(activeTabId)
  return (
    <div className="progressbar">
      <div className="progressbar-fill" style={{ width: `${percent}%` }} />
    </div>
  )
}
