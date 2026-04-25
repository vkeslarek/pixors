# Pixors Frontend Refactor Plan

**Audience:** an implementing model/engineer.
**Goal:** make every UI component **reactive to the engine** with **zero prop‑drilling for engine state**, and give the codebase three reusable primitives — `subscribe`, `dispatch`, `waitFor` — so future features compose without writing new bespoke hooks.
**Constraint:** **no behavior change.** Same WebSocket protocol, same render output, same UX. Only the wiring changes.

---

## 1. Why refactor — concrete problems in the current code

Read alongside `src/App.tsx`, `src/engine/hooks.ts`, `src/engine/client.ts`, `src/components/*`.

### 1.1 App.tsx is a god‑component

`App.tsx:36–78` calls **seven** engine hooks (`useEngineClient`, `useEngineConnection`, `useEngineTabs`, `useEngineTools`, `useEngineCommands`, `useEngineViewportState`, `useLoadingProgress`) and then **prop‑drills** their results into every child:

- `MenuBar` gets `activeTabName`, `onOpenFile`, `onExport` (`App.tsx:118`)
- `TabBar` gets `tabs`, `activeTabId`, `onTabClick`, `onTabClose`, `onTabAdd` (`App.tsx:127`)
- `Toolbar` gets `activeTool`, `onToolSelect` (`App.tsx:125`)
- `Viewport` gets `tabId`, `imageWidth`, `imageHeight`, `activeTool`, `connected`, `sendCommand`, `onMouseMove` (`App.tsx:134`)
- `StatusBar` gets `activeTool`, `zoom`, `layerCount`, `connected`, `error` (`App.tsx:163`)
- `Sidebar` gets **15 props** (`App.tsx:144`)

Result:
- App re‑renders on **every** engine event (tab created, viewport pan, load progress, tool change), which re‑renders the entire tree even though only one leaf component cares about a given event.
- Adding a new component that needs e.g. `activeTabId` means: add a hook in App, add a prop, drill down. Not scalable.

### 1.2 Each domain is a bespoke hook duplicating the same pattern

`hooks.ts:51–102` (`useEngineTabs`), `hooks.ts:104–110` (`useEngineTools`), `hooks.ts:112–124` (`useEngineViewportState`), `hooks.ts:126–148` (`useLoadingProgress`) all follow the same recipe:

```ts
const [x, setX] = useState(initial)
useEngineEvent('some_event', msg => setX(...))
return { x }
```

This is the wrong abstraction. It means:
- The store of truth is **per‑hook‑instance**. If two components both call `useEngineTabs`, they each maintain their own copy of `tabs[]` and each subscribes to the WS independently. Wasteful, and divergence is possible if event ordering differs.
- A new domain (selection, history, layers, adjustments) requires writing a new bespoke hook every time.
- There is no way to **read** state imperatively (e.g., from inside an event handler) without going through React lifecycle.

### 1.3 `useEngineCommands` returns a fresh object every render

`hooks.ts:150–168` returns a fresh `{ sendCommand, createTab, ... }` object each call. Consumers put `cmds` in `useEffect` dependency arrays (`App.tsx:60`, `App.tsx:78`) — the effect re‑runs every render. Today it happens to be idempotent, tomorrow it won't be.

### 1.4 No `waitFor` primitive

`createTabAndOpen` in `hooks.ts:157–166` hand‑rolls a one‑shot subscriber:

```ts
const onTabCreated = (msg) => {
  engineClient.sendCommand({ type: 'activate_tab', tab_id: msg.tab_id })
  engineClient.sendCommand({ type: 'open_file', tab_id: msg.tab_id, path })
  engineClient.off('tab_created', onTabCreated)
}
engineClient.on('tab_created', onTabCreated)
engineClient.sendCommand({ type: 'create_tab' })
```

Every multi‑step command flow will need this pattern. With no helper:
- Easy to forget the `off()` → leak.
- No timeout / cancellation.
- No error path (what if `create_tab` errors?).
- Boilerplate.

### 1.5 Side‑channel custom DOM events to dodge React

`Viewport.tsx:141` dispatches `window.dispatchEvent(new CustomEvent('mouse_pos', …))` and `StatusBar.tsx:25–32` listens for it. The reason (`StatusBar.tsx:14–21` comment) is that updating App‑level state on every mousemove would re‑render everything. **This is a symptom of 1.1**: with a proper external store + per‑slice subscription, the StatusBar can update at full mouse rate without touching any other component, and the custom event hack disappears.

Same root cause: `lastMousePosRef` throttle to 50 ms in `Viewport.tsx:131` — a workaround for the same problem.

### 1.6 Raw binary tile decoding lives in a UI component

`ViewportCanvas2D.tsx:174–207` does WS binary parsing (`engineClient.onBinary` + DataView offsets) inside a render hook. The WS protocol is leaking into the view layer. If the wire format changes — every hook that consumes binary frames must be updated. Decoding belongs in the engine client, not in the canvas.

### 1.7 UI state, engine state, and mock state are mixed in App

`App.tsx:48–51` keeps `layers`, `activeLayerId`, `adjustments`, `panelsOpen` as React local state alongside engine state. When phases 6+ wire layers into the engine, this code will need to be re‑plumbed everywhere. There's no boundary to swap.

### 1.8 Connect side‑effect runs on render, not on mount of the app shell

`hooks.ts:5–9` (`useEngineClient`) calls `engineClient.connect()` from inside a component. Whoever happens to render first triggers the connection. Connection lifecycle should be owned by the bootstrap, not by a component.

---

## 2. Target architecture

Three layers, sharp boundaries.

```
┌─────────────────────────────────────────────────────────┐
│ Components (TabBar, Viewport, Sidebar, StatusBar, …)    │
│   - Read store via selectors:  useEngine(s => s.tabs)   │
│   - Act via:                   engine.dispatch(cmd)     │
│   - No engine props. No prop drilling for engine data.  │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │ selector subscriptions
                          ▼
┌─────────────────────────────────────────────────────────┐
│ Engine Store (single source of truth, reactive)         │
│   slices: connection, tabs, activeTabId, tool,          │
│           viewport, loading, lastError                  │
│   reducer: applyEvent(state, event)                     │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │ events
                          ▼
┌─────────────────────────────────────────────────────────┐
│ Engine Facade  (engine.ts — single public API)          │
│   .dispatch(cmd)           send command                 │
│   .subscribe(type, cb)     low‑level event sub          │
│   .waitFor(pred, opts)     promise on next match event  │
│   .request(cmd, predicate) dispatch+waitFor combo       │
│   .onBinary(cb)            tile bitmap stream           │
│   .getState()              imperative state read        │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ EngineClient (existing, mostly unchanged)               │
│   WS connect/reconnect, msgpack codec, binary frames    │
└─────────────────────────────────────────────────────────┘
```

Key ideas:

1. **One store, many selectors.** Components subscribe to the slice they need. A component that reads `activeTool` does not re‑render when `viewport.zoom` changes.
2. **One reducer.** Every engine event flows through `applyEvent(state, event)` — a switch statement. New event type = new case. No new hook needed.
3. **Three primitives** — `dispatch`, `subscribe`, `waitFor` — replace bespoke per‑domain hooks. Multi‑step flows compose them.
4. **Components are reactive and self‑wired.** A component imports the store and the facade. It does not receive engine state via props. Adding a new component means writing it, not also editing App.
5. **Mock UI state (layers, adjustments) goes into a separate `uiStore`** — same shape, separate file, ready to swap to engine‑backed when phase 6 lands.

---

## 3. Implementation steps

Implement in this order. Each step compiles, tests pass, app still runs. Do not skip ahead.

### Step 1 — Add the store dependency

Use **zustand** (1.1 KB gzipped, idiomatic, supports `useSyncExternalStore` selectors).

```bash
cd pixors-ui && npm install zustand
```

**Why zustand and not Redux / Jotai / hand‑rolled?**
- Redux: too much ceremony for this scale; reducers + actions + middleware exceed the surface we need.
- Jotai: atom‑per‑slice fits, but we want one canonical reducer driven by engine events — atoms scatter that.
- Hand‑rolled `useSyncExternalStore`: ~30 lines, viable, but zustand already does it correctly with selector memoization, equality functions, and devtools. Cost is one dependency vs maintenance of a tiny utility.

If the user later objects to the dependency, the migration to a hand‑rolled store is mechanical (replace `create((set, get) => …)` with a tiny `createStore` helper). The component‑level selector API stays identical.

### Step 2 — Create `src/engine/store.ts` (the engine store)

```ts
import { create } from 'zustand'
import type { EngineEvent, UITab } from './types'

const TAB_COLORS = ['#ff4d4d', '#4dff4d', '#4d4dff', '#ffff4d', '#ff4dff', '#6ffff']

export interface EngineState {
  /**
   * Single source of connection truth, fed by engineClient.onConnection.
   * The `status` field on `session_state` is redundant with this and is
   * intentionally NOT mirrored into the store — see reducer note below.
   */
  connected: boolean
  sessionId: string

  tabs: UITab[]
  activeTabId: string | null

  tool: string

  // Per‑tab viewport state. Map keyed by tab_id.
  viewport: Record<string, { zoom: number; panX: number; panY: number }>

  // Per‑tab loading state.
  loading: Record<string, { active: boolean; percent: number }>

  lastError: string | null
}

export const useEngineStore = create<EngineState>(() => ({
  connected: false,
  sessionId: '',
  tabs: [],
  activeTabId: null,
  tool: 'Select',
  viewport: {},
  loading: {},
  lastError: null,
}))

// Reducer — single switch over EngineEvent.
// Pure: takes prev state, returns next. zustand `setState` will diff‑apply.
export function applyEvent(prev: EngineState, ev: EngineEvent): Partial<EngineState> {
  switch (ev.type) {
    case 'session_state':
      // ev.status intentionally ignored — `connected` is sourced from
      // engineClient.onConnection (transport truth), not from a payload field.
      return {
        sessionId: ev.session_id,
        activeTabId: ev.active_tab_id,
        tabs: ev.tabs.map((td, i) => ({
          id: td.id,
          name: td.name,
          color: TAB_COLORS[i % TAB_COLORS.length],
          modified: false,
          hasImage: td.has_image,
          width: td.width,
          height: td.height,
        })),
      }
    case 'tab_created': {
      if (prev.tabs.find(t => t.id === ev.tab_id)) return {}
      return {
        tabs: [...prev.tabs, {
          id: ev.tab_id,
          name: ev.name,
          color: TAB_COLORS[prev.tabs.length % TAB_COLORS.length],
          modified: false,
          hasImage: false,
          width: 0,
          height: 0,
        }],
      }
    }
    case 'tab_closed': {
      const tabs = prev.tabs.filter(t => t.id !== ev.tab_id)
      const activeTabId = prev.activeTabId === ev.tab_id
        ? (tabs.at(-1)?.id ?? null)
        : prev.activeTabId
      return { tabs, activeTabId }
    }
    case 'tab_activated':
      return { activeTabId: ev.tab_id }
    case 'image_loaded': {
      // Tolerate out‑of‑order: if tab unknown, create minimal entry.
      const exists = prev.tabs.find(t => t.id === ev.tab_id)
      const tabs = exists
        ? prev.tabs.map(t => t.id === ev.tab_id
            ? { ...t, hasImage: true, width: ev.width, height: ev.height } : t)
        : [...prev.tabs, {
            id: ev.tab_id, name: '(loading)',
            color: TAB_COLORS[prev.tabs.length % TAB_COLORS.length],
            modified: false, hasImage: true, width: ev.width, height: ev.height,
          }]
      return {
        tabs,
        loading: { ...prev.loading, [ev.tab_id]: { active: false, percent: 100 } },
      }
    }
    case 'image_load_progress':
      return {
        loading: { ...prev.loading, [ev.tab_id]: { active: true, percent: ev.percent } },
      }
    case 'image_closed':
      return {
        loading: { ...prev.loading, [ev.tab_id]: { active: false, percent: 0 } },
      }
    case 'tool_changed':
      return { tool: ev.tool }
    case 'viewport_updated':
      return {
        viewport: {
          ...prev.viewport,
          [ev.tab_id]: { zoom: ev.zoom, panX: ev.pan_x, panY: ev.pan_y },
        },
      }
    case 'error':
      return { lastError: ev.message }
    default:
      return {}
  }
}
```

Rationale: a pure reducer is testable, debuggable (log every event + diff), and trivially extensible — new event = new case.

### Step 3 — Create `src/engine/engine.ts` (the facade)

```ts
import { engineClient } from './client'
import type { EngineCommand, EngineEvent } from './types'
import { useEngineStore, applyEvent } from './store'

type EventType = EngineEvent['type']
type EventOf<T extends EventType> = Extract<EngineEvent, { type: T }>

class Engine {
  private booted = false

  /** Call once at app bootstrap (main.tsx). Idempotent. */
  boot() {
    if (this.booted) return
    this.booted = true

    // Single wildcard funnel: every event hits the reducer + handles side effects.
    engineClient.onAnyEvent(this.onAny)

    engineClient.onConnection(connected => {
      useEngineStore.setState({ connected })
    })

    useEngineStore.setState({ sessionId: engineClient.sessionId })

    engineClient.connect()
  }

  private onAny = (ev: EngineEvent) => {
    // Side effect: heartbeat auto‑reply (moved out of EngineClient constructor).
    if (ev.type === 'heartbeat') {
      engineClient.sendCommand({ type: 'heartbeat' })
      return // heartbeat does not feed the reducer
    }
    useEngineStore.setState(prev => applyEvent(prev, ev))
  }

  /** Send a command. Stable reference, safe in deps arrays. */
  dispatch = (cmd: EngineCommand) => engineClient.sendCommand(cmd)

  /** Low‑level: subscribe to a specific event type. Returns unsubscribe. */
  subscribe = <T extends EventType>(type: T, cb: (e: EventOf<T>) => void) =>
    engineClient.on(type, cb as any)

  /** Resolve on the next event matching predicate, or reject on timeout. */
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
      const off = engineClient.on(type, (e: any) => {
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

  /** Dispatch + waitFor in one. The 90% case. */
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

  /** Imperative read — for use inside event handlers, not in render. */
  getState = () => useEngineStore.getState()

  /** Tile binary stream — passthrough for the viewport. */
  onBinary = engineClient.onBinary.bind(engineClient)
}

export const engine = new Engine()
```

#### 3a. Wildcard subscription on `EngineClient`

`EngineClient` currently only supports per‑type listeners (`client.ts:126`). Add a wildcard:

```ts
// client.ts
private wildcardListeners: Set<(e: EngineEvent) => void> = new Set()
public onAnyEvent(cb: (e: EngineEvent) => void) {
  this.wildcardListeners.add(cb)
  return () => { this.wildcardListeners.delete(cb) }
}
private emit(event: EngineEvent) {
  for (const cb of this.wildcardListeners) cb(event)
  // existing per‑type dispatch unchanged
  ...
}
```

Then `engine.boot()` uses `engineClient.onAnyEvent(this.onAny)` instead of subscribing per type. Per‑type `subscribe` still works for component‑local one‑off needs.

### Step 4 — Bootstrap in `main.tsx` (fused with Step 3)

`engine.boot()` and `main.tsx` wiring land in the **same commit** as Step 3. Reason: store (Step 2) without boot is inert — there's no value in landing it alone, and a half‑wired state encourages someone else to start using it before it's hooked up.

```ts
import { engine } from './engine/engine'
engine.boot()
ReactDOM.createRoot(...).render(<App />)
```

Delete `useEngineClient` (no longer needed; bootstrap owns the connection). Delete the auto‑heartbeat handler from `EngineClient` constructor and move it to `engine.boot()` — bootstrap concerns belong with the bootstrap.

**Factory form for testability:**

```ts
export function createEngine() { return new Engine() }
export const engine = createEngine()
```

Tests instantiate a fresh `Engine` with a fake `EngineClient`. Production uses the singleton. Same call sites.

### Step 5 — Replace `src/engine/hooks.ts` with thin selectors

The old file becomes:

```ts
import { useEngineStore } from './store'
import { engine } from './engine'

// Convenience selectors. Components can also call useEngineStore(s => …) directly.
export const useTabs         = () => useEngineStore(s => s.tabs)
export const useActiveTabId  = () => useEngineStore(s => s.activeTabId)
export const useActiveTab    = () => useEngineStore(s => {
  const id = s.activeTabId
  return id ? s.tabs.find(t => t.id === id) ?? null : null
})
export const useTool         = () => useEngineStore(s => s.tool)
export const useConnected    = () => useEngineStore(s => s.connected)
export const useViewportFor  = (tabId: string | null) =>
  useEngineStore(s => tabId ? s.viewport[tabId] ?? null : null)
export const useLoadingFor   = (tabId: string | null) =>
  useEngineStore(s => tabId ? s.loading[tabId] ?? { active: false, percent: 0 }
                             : { active: false, percent: 0 })

// Re‑export facade for convenience.
export { engine }
```

That's the entire file. Every old `useEngine*` hook is gone — replaced by either a selector or by direct `engine.*` calls inside event handlers.

### Step 6 — Rewrite `createTabAndOpen` using `request`

In `engine.ts`, add domain helpers as small wrappers — these are optional sugar, not new abstractions:

```ts
// in Engine class
async createTabAndOpen(path: string) {
  const ev = await this.request({ type: 'create_tab' }, 'tab_created')
  this.dispatch({ type: 'activate_tab', tab_id: ev.tab_id })
  this.dispatch({ type: 'open_file', tab_id: ev.tab_id, path })
  return ev.tab_id
}
```

Compare to the original `hooks.ts:157–166`: no manual subscribe/unsubscribe, no leaks possible, awaitable, errorable, cancelable via `AbortSignal`.

### Step 6.5 — Incremental migration safety net (bridge hooks)

To avoid a big‑bang Step 7 where every component changes at once, land Steps 2–6 first with the **old hooks rewritten as thin selector wrappers** over the new store:

```ts
// hooks.ts — transitional shim
export const useEngineConnection = () => useEngineStore(s => s.connected)
export const useEngineTabs = () => ({
  tabs: useEngineStore(s => s.tabs),
  activeTabId: useEngineStore(s => s.activeTabId),
})
export const useEngineTools = () => ({ activeTool: useEngineStore(s => s.tool) })
export const useEngineCommands = () => ({
  sendCommand: engine.dispatch,
  createTab: () => engine.dispatch({ type: 'create_tab' }),
  closeTab: (id: string) => engine.dispatch({ type: 'close_tab', tab_id: id }),
  activateTab: (id: string) => engine.dispatch({ type: 'activate_tab', tab_id: id }),
  selectTool: (tool: string) => engine.dispatch({ type: 'select_tool', tool }),
  createTabAndOpen: (path: string) => engine.createTabAndOpen(path),
})
// ... rest of old API
```

App keeps working unchanged. Then Step 7 migrates components **one at a time**, each commit small and reviewable. After all components migrated, delete the shim. Big‑bang risk gone.

For each component, **delete the engine‑related props** and **read from the store**. Examples below show the diff‑shape — implementer should apply the same pattern across the file.

#### 7.1 `Toolbar.tsx`

Before (`App.tsx:125`): `<Toolbar activeTool={activeTool} onToolSelect={cmds.selectTool} />`
After: `<Toolbar />`

```ts
// Toolbar.tsx
import { useTool, engine } from '../engine'

export function Toolbar() {
  const activeTool = useTool()
  const onToolSelect = (id: string) => engine.dispatch({ type: 'select_tool', tool: id })
  // ...rest unchanged
}
```

#### 7.2 `TabBar`

Before: 5 props. After: zero engine props.

```ts
export function TabBar() {
  const tabs = useTabs()
  const activeTabId = useActiveTabId()
  // ...JSX uses engine.dispatch directly for click/close/add
}
```

#### 7.3 `StatusBar`

Before: `activeTool`, `zoom`, `connected`, `error`, plus the `mouse_pos` CustomEvent dance.
After: read from store. **Delete the custom DOM event entirely.** Mouse position is local UI concern — keep it inside `Viewport`/`StatusBar` via a tiny dedicated `useUIStore` slice (`mousePos: {x,y}`), or keep the CustomEvent for now (it does not hurt) but understand that with selector subscriptions the original perf concern (full‑app re‑render) is gone — `setState({mousePos})` on a separate store would only re‑render `StatusBar`. Recommend removing the CustomEvent in a follow‑up once the rest is in.

```ts
export function StatusBar() {
  const activeTool = useTool()
  const zoom = useEngineStore(s => {
    const id = s.activeTabId
    return id ? s.viewport[id]?.zoom ?? 1 : 1
  })
  const connected = useConnected()
  const error = useEngineStore(s => s.lastError)
  const layerCount = useUIStore(s => s.layers.length) // see §8
  // ...
}
```

#### 7.3b — Mouse position migration

Move `mousePos` from the `mouse_pos` CustomEvent (`Viewport.tsx:141`, `StatusBar.tsx:25–32`) into a slice of `useUIStore`:

```ts
// uiStore.ts
mousePos: { x: 0, y: 0 },
setMousePos: (p) => set({ mousePos: p }),
```

Viewport's mousemove handler calls `useUIStore.getState().setMousePos({x,y})` directly. StatusBar reads via `useUIStore(s => s.mousePos)`. Selector subscription means only StatusBar re‑renders — the original perf concern that motivated the CustomEvent is gone. **Delete the CustomEvent and the 50ms throttle.** This is the last React‑bypass hack to remove.

#### 7.4 `Viewport`

Before: 7 props (`App.tsx:134`) + 5 `useEngineEvent` calls (`Viewport.tsx:189–214`). After: zero engine props, every `useEngineEvent` becomes `engine.subscribe` inside `useEffect`.

Migration template (apply to all 5 subscriptions in `Viewport.tsx`):

```ts
export function Viewport() {
  const tabId = useActiveTabId()
  const tab = useActiveTab()
  const activeTool = useTool()
  const connected = useConnected()
  const imageWidth = tab?.width
  const imageHeight = tab?.height

  const { canvasRef, isReady, fit, pan, zoom, getCamera, hasAllTiles } =
    useCanvas2DViewport(tabId)

  // ...debounce/pendingRef/lastMousePosRef refs unchanged...

  const requestTiles = (force = false) => { /* unchanged */ }
  const requestDebounced = () => { /* unchanged */ }

  // useEngineEvent('image_loaded', ...)  →
  useEffect(() => {
    return engine.subscribe('image_loaded', (msg) => {
      if (msg.tab_id !== tabId || !isReady || !canvasRef.current) return
      fit(msg.width, msg.height)
      requestTiles()
    })
  }, [tabId, isReady, fit])

  // useEngineEvent('mip_level_ready', ...)  →
  useEffect(() => {
    return engine.subscribe('mip_level_ready', (msg) => {
      if (msg.tab_id === tabId && !pendingRef.current) {
        pendingRef.current = true
        requestTiles()
      }
    })
  }, [tabId])

  // useEngineEvent('tiles_complete', ...)  →
  useEffect(() => {
    return engine.subscribe('tiles_complete', () => { pendingRef.current = false })
  }, [])

  // useEngineEvent('tiles_dirty', ...)  →
  useEffect(() => {
    return engine.subscribe('tiles_dirty', (msg) => {
      if (msg.tab_id === tabId) { pendingRef.current = false; requestTiles(true) }
    })
  }, [tabId])

  // sendCommand prop is gone — call engine.dispatch inline inside requestTiles().
  // (Replace `sendCommand({...})` at Viewport.tsx:92 with `engine.dispatch({...})`.)
}
```

Pattern: each `useEngineEvent` → one `useEffect` that returns the unsubscribe. Dependencies are exactly what the handler closure reads from React state — usually just `tabId`. The handler reads everything else from refs, so the effect doesn't re‑subscribe on every render.

**Tile binary subscription** (`ViewportCanvas2D.tsx:174`): one‑line swap, `engineClient.onBinary` → `engine.onBinary`. Parser body unchanged.

The internal `useCanvas2DViewport(tabId)` hook stays — it is a legitimate encapsulated concern (camera + tile cache + RAF loop). The WS binary handling (`ViewportCanvas2D.tsx:174–207`) **stays in place** — Viewport is the only consumer of tile bytes, moving the parser to the facade now would be premature abstraction. **One line change only:** swap `engineClient.onBinary(...)` → `engine.onBinary(...)` so the facade is the single import point (centralized for testing/mocking). If a second consumer ever appears (thumbnail panel, mini‑map), extract the parser then.

#### 7.5 `MenuBar`, `ActivityBar`, `Sidebar`, `ProgressBar`

Same treatment. `MenuBar` reads `useActiveTab()?.name` instead of receiving `activeTabName`. `onOpenFile` becomes `() => engine.dispatch({ type: 'open_file_dialog', tab_id: activeTabId ?? undefined })`.

`ProgressBar` today receives `percent` from App (`App.tsx:162`). After: zero props.

```ts
export function ProgressBar() {
  const activeTabId = useActiveTabId()
  const { percent, active } = useLoadingFor(activeTabId)
  if (!active && percent >= 100) return null
  // ...JSX unchanged
}
```

App renders `<ProgressBar />` with no props.

#### 7.6 The new `App.tsx`

After this step, `App.tsx` collapses to a layout shell — no engine hooks, no engine props passed down:

```ts
export default function App() {
  // Global keymap stays at App level (it's an app‑shell concern).
  useKeymap()

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar />
        <div className="workspace">
          <ActivityBar />
          <Toolbar />
          <div className="canvas-column">
            <TabBar />
            <Viewport />
          </div>
          <Sidebar />
        </div>
        <ProgressBar />
        <StatusBar />
      </div>
    </Tooltip.Provider>
  )
}
```

`useKeymap` is a small hook that maps keys to `engine.dispatch`. Currently inline in `App.tsx:62–78`; extract to `src/hooks/useKeymap.ts`.

This is the headline win: every component is now reactive to exactly the slice it cares about, with no plumbing in App.

### Step 8 — Move mock UI state to `src/ui/uiStore.ts`

Create a sibling store for **mock UI state** (layers, adjustments, panelsOpen, activeLayerId). Same zustand pattern. `Sidebar` reads from it directly.

```ts
// ui/uiStore.ts
import { create } from 'zustand'
import type { Layer, Adjustment } from '../types'

interface UIState {
  layers: Layer[]
  activeLayerId: string
  adjustments: Adjustment[]
  panelsOpen: { hist: boolean; props: boolean; adj: boolean; layers: boolean }
  // mutations:
  toggleVisibility: (id: string) => void
  toggleLock: (id: string) => void
  // ...
}

export const useUIStore = create<UIState>((set, get) => ({ /* … */ }))
```

`Sidebar` no longer takes props — reads/mutates `useUIStore` directly. When phase 6 wires layers into the engine, swap `useUIStore` for engine‑backed slices; the **component code does not change** because the selector signature is identical.

This is the second headline win: the mock/real boundary is one file, not a tree.

### Step 9 — Cleanup

- Delete `useEngineCommands` (replaced by `engine.dispatch` + `engine.createTabAndOpen`).
- Delete `useEngineSession` (replaced by selectors).
- Remove the `mouse_pos` CustomEvent (optional, follow‑up): replace with a `mousePos` slice in `useUIStore` set from the Viewport's mousemove handler. Per‑slice subscription means this no longer causes app re‑renders.
- Move `engineClient.heartbeat` self‑reply (`client.ts:48–52`) out of the constructor and into `engine.boot()`.
- Drop `DEBUG`/`DEBUG_WS` flags in favor of a single `engine.setDebug(true)` toggle on the facade — easier to flip from the console.

### Step 10 — Verification

Functional smoke test (manual, ~5 min):

1. `npm run dev`, open browser. WS connects (`Connected` indicator green).
2. Open a file via menu / `Ctrl+O`. Tab appears, image renders.
3. Pan + zoom. Status bar zoom updates. Tiles stream.
4. Open second tab, switch tabs. Active state switches correctly.
5. Close tab. Active tab fallback selects the previous one.
6. Disconnect engine (kill backend) — `Disconnected` indicator. Reconnect — auto‑recovers and re‑hydrates `session_state`.

Component re‑render check (sanity, optional): install React DevTools profiler. Pan the viewport. Confirm `MenuBar`, `Sidebar`, `Toolbar` do **not** re‑render (today they do, on every pan because of `engineZoom` in App).

---

## 4. What the three primitives buy you

`subscribe`, `dispatch`, `waitFor` cover every interaction shape the engine produces. Examples:

| Use case | Old code | New code |
|---|---|---|
| Listen for `tool_changed` and update local state | `useEngineEvent('tool_changed', e => setTool(e.tool))` per component | `useTool()` (selector) — store does it once |
| Send a command | `cmds.selectTool('brush')` (object recreated each render) | `engine.dispatch({type:'select_tool', tool:'brush'})` (stable) |
| Send + wait for ack | hand‑roll subscribe/unsubscribe | `await engine.request(cmd, 'image_loaded', e => e.tab_id === id)` |
| Multi‑step flow | nested callbacks, manual `off()` | `async` function with `await`s, `AbortSignal` for cancellation |
| One‑off DOM‑event‑style listen | `engineClient.on('image_loaded', cb)` directly | `engine.subscribe('image_loaded', cb)` (same signature, but routed through facade so testing/mocking is centralized) |

When phase 3 (Operations) lands and there's a long‑running `apply_op` command that streams `op_progress` events and ends in `op_complete` or `op_error`, the wire‑up is one async function:

```ts
async function applyBrightness(tabId: string, value: number, signal: AbortSignal) {
  const job = await engine.request(
    { type: 'apply_op', tab_id: tabId, op: 'brightness', value },
    'op_started',
  )
  const onProgress = engine.subscribe('op_progress', e => {
    if (e.job_id === job.job_id) useUIStore.getState().setProgress(e.percent)
  })
  try {
    return await engine.waitFor('op_complete', e => e.job_id === job.job_id, { signal })
  } finally {
    onProgress()
  }
}
```

No new hook. No props. Composable.

---

## 5. Non‑goals (explicit)

- **No Redux Toolkit, no Saga, no MobX.** Overkill at this scale.
- **No big React Query / SWR.** The engine is push‑based over WS, not request‑response over HTTP.
- **No move to Solid / Preact signals.** Out of scope; would change too much.
- **No CSS / styling refactor.** Tokens, fonts, layout untouched.
- **No protocol changes.** Wire format and command/event shapes are the contract — frozen.
- **No new features.** Phase 3 ops, phase 6 layers, etc., are out of scope. The plan only changes wiring.

---

## 6. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Selector over‑subscription (component re‑renders on unrelated changes) | Use zustand's `shallow` equality fn for object selectors; prefer atomic selectors (`s => s.tool`) over composite ones. |
| Event ordering bugs in the reducer (e.g. `tab_activated` before `tab_created`, `viewport_updated` before `image_loaded`) | Every reducer case must tolerate missing predecessor state. **Mandatory tests:** `tab_activated` for unknown id (sets id, list filter handles render); `image_loaded` for unknown tab (creates minimal entry); `viewport_updated` for unknown tab (writes to map regardless); `tab_closed` after fallback selection. Add one out‑of‑order test per event type. |
| `waitFor` listener leaks under race conditions (timeout + event same tick, double‑resolve, abort during cleanup) | `done` flag guards every resolution path; `cleanup` is idempotent. Mandatory tests in §8 acceptance criteria. |
| Reconnect re‑hydration | `engineClient.onopen` already sends `get_session_state` (`client.ts:67–68`). The reducer's `session_state` case fully replaces `tabs` and `activeTabId`. Verified path. |
| Hidden coupling on stable `cmds` reference | `engine.dispatch` is a class‑bound method — stable for the app lifetime. Safe in any deps array. |
| Tile binary path regressions | Decoding logic stays byte‑identical; only the entry point moves from `engineClient.onBinary` to `engine.onBinary` (passthrough). Same buffer math (`ViewportCanvas2D.tsx:177–198`). |

---

## 7. File‑level change inventory

**New:**
- `src/engine/store.ts` — zustand store + `applyEvent` reducer.
- `src/engine/engine.ts` — facade with `dispatch` / `subscribe` / `waitFor` / `request`.
- `src/ui/uiStore.ts` — mock UI state store (layers, adjustments, panels, mouse position).
- `src/hooks/useKeymap.ts` — extracted keymap.

**Rewritten (small):**
- `src/engine/hooks.ts` — now ~30 lines of selectors. Delete all old `useEngine*` bespoke hooks.
- `src/engine/client.ts` — add `onAnyEvent` wildcard (~10 LoC), remove constructor heartbeat self‑reply.
- `src/main.tsx` — call `engine.boot()` once before render.
- `src/App.tsx` — collapses to layout shell. ~30 LoC.

**Modified (mechanical):**
- `src/components/MenuBar.tsx` — drop engine props, read store.
- `src/components/Toolbar.tsx` — drop engine props, read store.
- `src/components/Viewport.tsx` — drop engine props, read store.
- `src/components/Sidebar.tsx` — drop UI props, read `useUIStore`.
- `src/components/StatusBar.tsx` — drop engine props, read store. Optionally remove `mouse_pos` CustomEvent.
- `src/components/ProgressBar.tsx` — read `useLoadingFor(activeTabId)` directly.
- `src/components/viewport/ViewportCanvas2D.tsx` — switch `engineClient.onBinary` → `engine.onBinary`.

**Deleted:**
- `useEngineClient`, `useEngineSession`, `useEngineTabs`, `useEngineTools`, `useEngineCommands`, `useEngineViewportState`, `useLoadingProgress` from `hooks.ts` — replaced by selectors and the facade.

---

## 8. Acceptance criteria for the implementing model

1. `App.tsx` body has **zero** engine `use*` calls and **zero** engine state passed as props to children.
2. Every component that depends on engine state imports from `src/engine` (selector) or calls `engine.dispatch` directly. No engine WebSocket types are imported in components except the Viewport.
3. `engine.dispatch`, `engine.subscribe`, `engine.waitFor`, `engine.request` exist, are exported from `src/engine`, and have unit tests:
   - `dispatch` calls `engineClient.sendCommand` once.
   - `subscribe` returns a working unsubscribe.
   - `waitFor` resolves on match.
   - `waitFor` rejects on timeout and removes the listener.
   - `waitFor` rejects on abort and removes the listener.
   - `waitFor` honors a pre‑aborted signal synchronously.
   - `waitFor` does not double‑resolve when timeout and matching event fire in the same tick (cleanup idempotent).
   - `request` rejects if `waitFor` rejects (no leaked listener).
4. The reducer (`applyEvent`) has one test per `EngineEvent` case verifying the slice it produces.
5. `npm run dev` runs. Manual smoke test in §3 step 10 passes.
6. `npm run build` succeeds with zero TypeScript errors.
7. No new top‑level dependencies besides `zustand`.

---

## 9. Out‑of‑scope follow‑ups (file as TODOs, do not do now)

- Replace `mouse_pos` CustomEvent with `useUIStore` slice (see §7.3).
- Add a devtools panel: subscribe to `engine.subscribe`‑with‑wildcard and render the last 100 events for debugging.
- Extract tile cache + camera into a non‑React module (currently entangled with `useCanvas2DViewport`); React layer becomes a thin adapter.
- When phase 3 ships, add `engine.applyOp(...)` helper using `request` + `waitFor` pattern.
- Migrate `useUIStore.layers` / `adjustments` to engine‑backed when phase 6 lands. Component code unchanged.
