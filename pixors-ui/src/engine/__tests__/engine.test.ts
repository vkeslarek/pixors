import { describe, it, expect, vi } from 'vitest'
import { createEngine } from '@/engine/engine'
import type { EngineEvent, EngineCommand } from '@/engine/types'

// Minimal fake EngineClient that we can control synchronously.
function fakeClient() {
  const wildcards: Set<(e: EngineEvent) => void> = new Set()
  const perType: Map<string, Set<Function>> = new Map()
  const connectionCbs: Set<Function> = new Set()
  const binaryCbs: Set<Function> = new Set()
  let sent: EngineCommand[] = []

  return {
    sent: () => sent,
    resetSent: () => { sent = [] },
    emit(e: EngineEvent) {
      for (const cb of wildcards) cb(e)
      perType.get(e.type)?.forEach(cb => cb(e))
    },
    onAnyEvent(cb: (e: EngineEvent) => void) { wildcards.add(cb); return () => wildcards.delete(cb) },
    on(type: string, cb: Function) {
      if (!perType.has(type)) perType.set(type, new Set())
      perType.get(type)!.add(cb)
      return () => perType.get(type)?.delete(cb)
    },
    onBinary(cb: Function) { binaryCbs.add(cb); return () => binaryCbs.delete(cb) },
    onConnection(cb: Function) { connectionCbs.add(cb); return () => connectionCbs.delete(cb) },
    sendCommand(cmd: EngineCommand) { sent.push(cmd) },
    connect() {},
    connected: false,
    sessionId: 'test-session',
  } as any
}

describe('engine.dispatch', () => {
  it('sends a command to the client', () => {
    const client = fakeClient()
    const engine = createEngine(client)
    engine.dispatch({ type: 'create_tab' })
    expect(client.sent()).toEqual([{ type: 'create_tab' }])
  })
})

describe('engine.subscribe', () => {
  it('calls callback on matching event and returns unsubscribe', () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const cb = vi.fn()
    const unsub = engine.subscribe('tool_changed', cb)
    client.emit({ type: 'tool_changed', tool: 'brush' } as EngineEvent)
    expect(cb).toHaveBeenCalledTimes(1)
    unsub()
    client.emit({ type: 'tool_changed', tool: 'pen' } as EngineEvent)
    expect(cb).toHaveBeenCalledTimes(1)
  })
})

describe('engine.waitFor', () => {
  it('resolves on matching event', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const p = engine.waitFor('tool_changed')
    client.emit({ type: 'tool_changed', tool: 'brush' } as EngineEvent)
    const result = await p
    expect(result).toMatchObject({ type: 'tool_changed', tool: 'brush' })
  })

  it('resolves with predicate', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const p = engine.waitFor('tool_changed', (e) => e.tool === 'brush')
    client.emit({ type: 'tool_changed', tool: 'pen' } as EngineEvent)
    client.emit({ type: 'tool_changed', tool: 'brush' } as EngineEvent)
    const result = await p
    expect(result.tool).toBe('brush')
  })

  it('rejects on timeout', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const p = engine.waitFor('tool_changed', () => true, { timeoutMs: 10 })
    await expect(p).rejects.toThrow('timed out')
  })

  it('rejects on abort', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const ctrl = new AbortController()
    const p = engine.waitFor('tool_changed', () => true, { signal: ctrl.signal })
    ctrl.abort()
    await expect(p).rejects.toThrow('aborted')
  })

  it('rejects immediately on pre-aborted signal', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const ctrl = new AbortController()
    ctrl.abort()
    const p = engine.waitFor('tool_changed', () => true, { signal: ctrl.signal })
    await expect(p).rejects.toThrow('aborted')
  })

  it('does not double-resolve when timeout fires and event arrives (cleanup idempotent)', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    // Use a longer timeout, resolve via event, then verify timeout doesn't reject later
    const p = engine.waitFor('tool_changed', () => true, { timeoutMs: 50 })
    client.emit({ type: 'tool_changed', tool: 'x' } as EngineEvent)
    const result = await p
    expect(result).toBeDefined()
    // Wait for timeout to fire (it won't because cleanup ran)
    await new Promise(r => setTimeout(r, 60))
  })

  it('removes listener after resolve (no leak)', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const cb = vi.fn()
    engine.subscribe('tool_changed', cb)
    const p = engine.waitFor('tool_changed')
    client.emit({ type: 'tool_changed', tool: 'x' } as EngineEvent)
    await p
    // Emit again — the waitFor listener should be gone, but the subscribe listener still fires
    client.emit({ type: 'tool_changed', tool: 'y' } as EngineEvent)
    expect(cb).toHaveBeenCalledTimes(2)
  })
})

describe('engine.request', () => {
  it('dispatches then waits', async () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const p = engine.request({ type: 'create_tab' }, 'tab_created')
    expect(client.sent()).toContainEqual({ type: 'create_tab' })
    client.emit({ type: 'tab_created', tab_id: 't1', name: 'a' } as EngineEvent)
    const result = await p
    expect(result.tab_id).toBe('t1')
  })
})

describe('engine.onBinary', () => {
  it('passes through to client', () => {
    const client = fakeClient()
    const engine = createEngine(client)
    const cb = vi.fn()
    expect(typeof engine.onBinary).toBe('function')
    const unsub = engine.onBinary(cb)
    expect(typeof unsub).toBe('function')
  })
})
