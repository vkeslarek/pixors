import { useEffect, useCallback, useSyncExternalStore } from 'react'
import type { EngineEvent, EngineCommand } from '@/engine/types'
import { engine } from '@/engine/engine'

type EventType = EngineEvent['type']

export function useEvent<T extends EventType>(
  type: T,
  handler: (ev: Extract<EngineEvent, { type: T }>) => void,
) {
  useEffect(() => {
    return engine.subscribe(type, handler)
  }, [type, handler])
}

export function useCommand<T extends EngineCommand['type']>(type: T) {
  return useCallback((params?: Omit<Extract<EngineCommand, { type: T }>, 'type'>) => {
    engine.dispatch({ type, ...params } as EngineCommand)
  }, [type])
}

export function useConnected() {
  return useSyncExternalStore(
    useCallback((cb: () => void) => engine.onConnection(cb), []),
    () => engine.connected,
  )
}
