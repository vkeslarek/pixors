# Pixors Frontend — Architecture & Patterns

## Directory Structure

```
pixors-ui/src/
├── engine/                      # Engine communication layer
│   ├── events.ts                # useEvent / useCommand / useConnected hooks
│   ├── engine.ts                # Engine class (dispatch, subscribe, request, waitFor)
│   ├── client.ts                # WebSocket client, msgpack, session
│   ├── types.ts                 # EngineCommand, EngineEvent unions
│   └── index.ts                 # Barrel export
│
├── components/                  # React components (one per domain)
│   ├── MenuBar.tsx              # Tab bar + file/view/window menus
│   ├── Toolbar.tsx              # Tool selector buttons
│   ├── Viewport.tsx             # Canvas + camera + tile requests
│   ├── StatusBar.tsx            # Tool / zoom / layer count / connection
│   ├── ProgressBar.tsx          # Image load progress bar
│   ├── panels/
│   │   └── LayersPanel.tsx      # Layer list (read-only for now)
│   └── viewport/
│       ├── ViewportCanvas2D.tsx # Canvas 2D renderer hook
│       └── LRUTileCache.ts      # Tile bitmap LRU cache
│
├── ui/                          # UI infrastructure (panel docking)
│   ├── uiStore.ts               # Panel layout Zustand (localStorage persisted)
│   ├── panelLayout.ts           # Layout types + defaults
│   ├── useDockDnd.ts            # Drag-and-drop target logic
│   └── useResizeHandle.ts       # Panel resize handle
│
├── App.tsx                      # Root component (layout assembly)
├── main.tsx                     # Entry point (engine.boot())
├── keymap.ts                    # Keyboard shortcuts
├── tokens.css                   # Design tokens + CSS reset
├── App.css                      # Application styles
└── types.ts                     # Legacy types (consider merging with engine/types.ts)
```

## Core Pattern

Every component owns its **own state** via React hooks. There is **no global store** for engine state — each component subscribes to the events it needs.

### 2 Hooks (from `@/engine/events`)

```typescript
import { useEvent, useCommand, useConnected } from '@/engine/events'
```

| Hook | Purpose | Example |
|------|---------|---------|
| `useEvent(type, handler)` | Subscribes to an engine event | `useEvent('tab_created', (ev) => ...)` |
| `useCommand(type)` | Returns a dispatch function | `const createTab = useCommand('create_tab')` |
| `useConnected()` | Returns connection status | `const connected = useConnected()` |

### Component Template

```typescript
import { useState } from 'react'
import { useEvent, useCommand } from '@/engine/events'

function MyComponent() {
  // 1. Local state
  const [data, setData] = useState<MyData | null>(null)

  // 2. Subscribe to engine events → update local state
  useEvent('my_state', (ev) => {
    setData(ev.some_field)
  })

  // 3. Command dispatcher
  const myCommand = useCommand('my_command')

  // 4. Render
  return (
    <button onClick={() => myCommand({ param: 42 })}>
      {data}
    </button>
  )
}
```

## How to Add a New Feature (Frontend + Backend)

This is the **1:1 component ↔ service** pattern. Adding a feature = adding files on both sides without touching existing code.

### Step 1 — Backend (`pixors-engine/src/server/service/`)

Create a new service file:

```rust
// pixors-engine/src/server/service/{name}.rs

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum {Name}Command {
    DoSomething { param: u32 },
    Get{Name}State,           // always include for reconnection sync
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum {Name}Event {
    SomethingHappened { result: String },
    {Name}State { ... },      // always include for reconnection sync
}

pub struct {Name}Service;

impl Service for {Name}Service { ... }
```

**Then register it — exactly 3 lines:**
1. `mod.rs`: `pub mod {name};`
2. `event_bus.rs`: Add `{Name}({Name}Command)` to `EngineCommand` and `EngineEvent`
3. `app.rs`: Add `{name}_service` field + dispatch arm in `route_command`

### Step 2 — Frontend (`pixors-ui/src/components/`)

Create a new component:

```typescript
// pixors-ui/src/components/{Name}View.tsx

import { useState } from 'react'
import { useEvent, useCommand } from '@/engine/events'

export function {Name}View() {
  const [state, setState] = useState<...>(null)

  // Sync on reconnect
  useEvent('{name}_state', (ev) => setState(ev))

  // Handle real-time updates
  useEvent('{name}_updated', (ev) => ...)

  // Send commands
  const doSomething = useCommand('do_something')

  return <div>...</div>
}
```

**Then add types — exactly 1 line per type in `engine/types.ts`:**

```typescript
// EngineCommand union — add 2 lines:
| { type: 'do_something'; param: number }
| { type: 'get_{name}_state' }

// EngineEvent union — add 2 lines:
| { type: '{name}_state'; ... }
| { type: 'something_happened'; result: string }
```

### Step 3 — Wire component into layout

Add `<{Name}View />` in `App.tsx` or `DockArea.tsx` layout.

### Checklist

- [ ] Backend service: `Command` + `Event` enums + `GetState` variant
- [ ] Backend: registered in `mod.rs`, `event_bus.rs`, `app.rs`
- [ ] Frontend component: local state + `useEvent` + `useCommand`
- [ ] Frontend types: command + event unions in `types.ts`
- [ ] Component wired into layout
- [ ] `cargo check` + `cargo clippy` green
- [ ] `npm run build` green

## Available Components

| Component | Subscription | Commands | State |
|-----------|-------------|----------|-------|
| `MenuBar.tsx` | `tab_state`, `tab_created`, `tab_closed`, `tab_activated`, `image_loaded` | `create_tab`, `close_tab`, `activate_tab` | tabs list, active tab |
| `Toolbar.tsx` | `tool_state`, `tool_changed` | `select_tool` | active tool |
| `Viewport.tsx` | `image_loaded`, `layer_changed`, `doc_size_changed`, `mip_level_ready`, `tiles_complete`, `tiles_dirty`, `tab_*`, `tool_*` | `request_tiles` | camera (pan/zoom), active tab |
| `StatusBar.tsx` | `tool_*`, `viewport_*`, `layer_state`, `image_loaded`, `doc_size_changed`, `error`, `tab_*` | — | tool, zoom, layer count, image size |
| `LayersPanel.tsx` | `layer_state`, `layer_changed`, `tab_*` | — | layers list |
| `ProgressBar.tsx` | `image_load_progress`, `image_loaded`, `image_closed`, `tab_*` | — | load percent |
| `App.tsx` | `error`, `tab_*` | — | toaster, keymap |

## Key Rules

1. **No global store for engine data.** Each component owns its state locally.
2. **useEvent for inbound.** Never import `engine.subscribe()` directly in components.
3. **useCommand for outbound.** Never import `engine.dispatch()` directly in components.
4. **Always add GetState.** Every service must support `GetState` so reconnection works.
5. **Types in types.ts only.** Protocol types go in `engine/types.ts`. UI types go in the component file.
6. **ui/uiStore persists layout.** Panel layout is the only global state (localStorage-backed).
