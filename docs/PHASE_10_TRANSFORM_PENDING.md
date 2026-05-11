# Phase 10 — Transform Model: Pending Work

This document tracks everything that exists in the `Transform` data model but is not yet
implemented in the render compiler or desktop UI.

---

## Compiler (`pixors-document/src/render/compiler.rs`)

### Operations

| Operation | Status | Notes |
|---|---|---|
| `Operation::Blur { radius }` | ✅ Done | `TileToNeighborhood → Blur` on GPU/CPU |
| `Operation::Exposure { stops }` | ❌ Todo | Requires a multiply-by-scalar GPU/CPU stage |

### InputScope

| Variant | Status | Notes |
|---|---|---|
| `InputScope::Layer` | ✅ Done | Default — reads the layer's own pixels |
| `InputScope::Below` | ❌ Todo | Requires the composed-below-current-layer intermediate result to be wired in |
| `InputScope::Reference(NodeId)` | ❌ Todo | `compile_reference` memoizes but currently hits `todo!()` |

### OutputMode

| Variant | Status | Notes |
|---|---|---|
| `OutputMode::Replace` | ✅ Done | Op result replaces the layer's pixel data |
| `OutputMode::Composite { blend, position }` | ❌ Todo | Op result is composited over/under the layer (drop shadows, outer glow) |

### Layer sources

| Source | Status | Notes |
|---|---|---|
| `PixelSource::PrimaryAsset` | ✅ Done | Reads from LZ4 tile cache |
| `PixelSource::SolidColor` | ❌ Todo | Needs a solid-color tile producer |

---

## Mutations (`pixors-document/src/mutation/impls.rs`)

The `AddTransform`, `RemoveTransform`, `UpdateTransformOp` mutations exist structurally but
are **not yet wired to the History** or dispatched as proper `DocumentMutation` actions.
Currently, `CommitBlur` in `controller.rs` mutates `LayerNode.transforms` directly (no undo).

| Task | Status |
|---|---|
| Wire `AddTransform` through `Dispatcher::dispatch()` so it gets recorded in `History` | ❌ Todo |
| Wire `RemoveTransform` same | ❌ Todo |
| Wire `UpdateTransformOp` same | ❌ Todo |
| `CommitBlur` handler: use `AddTransform`/`UpdateTransformOp` instead of direct mutation | ❌ Todo |

---

## UI

| Feature | Status | Notes |
|---|---|---|
| Blur commit writes to history (undo works) | ❌ Todo | Needs mutation wiring above |
| Add transform from New Filter panel | ❌ Todo | `NewFilter` panel exists but dispatches nothing |
| Remove transform button in Filters panel | ❌ Todo |
| Reorder transforms via drag | ❌ Todo |
| `Exposure` slider in Filters panel | ❌ Todo | Blocked on `Operation::Exposure` compiler |
| Show all transforms for active layer (not just first blur) | ❌ Todo | Filters panel currently assumes at most one Blur |

---

## Preview system

| Feature | Status | Notes |
|---|---|---|
| `session.active_preview` overlay path | ✅ Partial | `UpdatePreview`/`CancelPreview` exist, `CommitPreview` is no-op |
| Compiler preview: apply `preview_overrides` instead of document ops | ❌ Todo | `RenderRequest::up_to` field exists for partial renders |
| Preview for non-Blur ops | ❌ Todo | Blocked on Exposure + overlay compiler path |

---

## Deferred architectural work

- `InputScope::Below` requires the compiler to produce an intermediate composited result
  before the current layer and thread it through as an extra input to the transform chain.
- `OutputMode::Composite` requires a second Compose stage per transform that merges the
  op result back onto the layer using the transform's blend spec.
- `compile_reference` needs to look up the target `NodeId` in `doc.layers`, compile it
  in isolation, and memoize to prevent re-compilation.
