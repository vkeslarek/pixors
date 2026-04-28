import { engineClient, type EngineClient } from '@/engine/client'
import type { EngineCommand, EngineEvent } from '@/engine/types'

type EventType = EngineEvent['type']
type EventOf<T extends EventType> = Extract<EngineEvent, { type: T }>

class Engine {
  private client: EngineClient
  private booted = false

  constructor(client?: EngineClient) {
    this.client = client ?? engineClient
    this.onBinary = this.client.onBinary.bind(this.client)
  }

  get connected() { return this.client.connected }

  onConnection = (cb: (connected: boolean) => void) => this.client.onConnection(cb)

  boot() {
    if (this.booted) return
    this.booted = true

    this.client.onAnyEvent(ev => {
      if (ev.type === 'heartbeat') {
        this.client.sendCommand({ type: 'heartbeat' })
      }
    })

    this.client.connect()
  }

  dispatch = (cmd: EngineCommand) => this.client.sendCommand(cmd)

  subscribe = <T extends EventType>(type: T, cb: (e: EventOf<T>) => void) =>
    this.client.on(type, cb as any)

  waitFor<T extends EventType>(
    type: T,
    predicate: (e: EventOf<T>) => boolean = () => true,
    opts: { timeoutMs?: number; signal?: AbortSignal } = {},
  ): Promise<EventOf<T>> {
    const { timeoutMs = 5000, signal } = opts
    return new Promise((resolve, reject) => {
      let done = false
      const cleanup = () => {
        if (done) return
        done = true
        off()
        clearTimeout(timer)
        signal?.removeEventListener('abort', onAbort)
      }
      const off = this.client.on(type, (e: any) => {
        if (done || !predicate(e)) return
        cleanup()
        resolve(e)
      })
      const timer = setTimeout(() => {
        if (done) return
        cleanup()
        reject(new Error(`waitFor(${type}) timed out after ${timeoutMs}ms`))
      }, timeoutMs)
      const onAbort = () => {
        if (done) return
        cleanup()
        reject(new DOMException('aborted', 'AbortError'))
      }
      signal?.addEventListener('abort', onAbort)
      if (signal?.aborted) onAbort()
    })
  }

  async request<T extends EventType>(
    cmd: EngineCommand,
    expect: T,
    predicate?: (e: EventOf<T>) => boolean,
    opts?: { timeoutMs?: number; signal?: AbortSignal },
  ): Promise<EventOf<T>> {
    const p = this.waitFor(expect, predicate, opts)
    this.dispatch(cmd)
    return p
  }

  onBinary: (cb: (type: number, payload: Uint8Array, len: number) => void) => () => void

  async createTabAndOpen(path: string) {
    const ev = await this.request({ type: 'create_tab' }, 'tab_created')
    this.dispatch({ type: 'activate_tab', tab_id: ev.tab_id })
    this.dispatch({ type: 'open_file', tab_id: ev.tab_id, path })
    return ev.tab_id
  }
}

export function createEngine(client?: EngineClient) { return new Engine(client) }
export const engine = createEngine()
