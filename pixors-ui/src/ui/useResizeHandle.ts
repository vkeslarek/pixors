import { useCallback, useRef } from 'react'

/** Hook for custom resize handles. Returns onPointerDown handler. */
export function useResizeHandle(
  onResize: (delta: number) => void,
  axis: 'x' | 'y' = 'x'
) {
  const ref = useRef<{ start: number; active: boolean }>({ start: 0, active: false })

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault()
    e.stopPropagation()
    const el = e.currentTarget as HTMLElement
    el.setPointerCapture(e.pointerId)
    ref.current = { start: axis === 'x' ? e.clientX : e.clientY, active: true }
  }, [axis])

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    if (!ref.current.active) return
    const current = axis === 'x' ? e.clientX : e.clientY
    onResize(current - ref.current.start)
    ref.current.start = current
  }, [axis, onResize])

  const onPointerUp = useCallback(() => {
    ref.current.active = false
  }, [])

  return { onPointerDown, onPointerMove, onPointerUp }
}
