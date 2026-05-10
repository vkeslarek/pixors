# Known Bugs

---

## BUG-01 · Blur preview drops random tiles

**Symptom:** During fast blur preview (slider drag), some tiles render blurred correctly
while others disappear entirely, leaving black or transparent gaps across the viewport.
The pattern is non-deterministic — different tiles drop on each preview run.

**Screenshot:** `docs/assets/bug01-blur-tiles.png` (screenshot from 2026-05-09)

**Root cause (suspected):** The preview pipeline writes blurred tiles into the overlay
generation of `TileCache` via `TileCacheSink`. If the pipeline produces tiles out of
order, or if a previous preview's cancel races with the new pipeline's writes, some
tile slots in the overlay are never filled. The viewport then renders black for any
`TileCache` miss that falls back to nothing (base tiles are skipped when an overlay
generation is active).

**Fix direction:**
- On preview start, pre-populate all expected overlay tile slots with a sentinel
  (e.g. copy from base) before the pipeline writes blurred versions. This prevents
  the viewport seeing a partial overlay.
- Or: the viewport falls back to base tiles for any slot not yet written in the
  current overlay generation, instead of showing black.
- Either approach requires `TileCache::get()` to distinguish "overlay generation
  active but this slot not yet written" from "no tile exists at all".

**Priority:** High — visible corruption during the primary interactive operation.

---

## BUG-02 · Panning during preview discards blur tiles

**Symptom:** While a blur preview is active and the user pans or zooms the viewport,
the blurred tiles disappear and the viewport reverts to the unblurred base tiles (or
shows black). The user must release the slider and move it again to re-trigger preview.

**Root cause:** `handle_tick` / the viewport program detects a camera change, cancels
the current background pipeline (correct), and triggers `run_mip_fetch` (base tiles,
no blur). The blur preview is not re-run with the new tile range.

**Fix direction:** When a blur preview is the active operation (slider is held), camera
changes should re-run `run_blur_preview` with the new `TileRange`, not `run_mip_fetch`.
Track "preview active" state in `App` (e.g. `active_preview: Option<BlurPreviewParams>`).
On camera change, if `active_preview.is_some()`, restart blur preview for new range.
If "preview active" state is not yet a first-class concept, this is a good forcing
function for `FilterPanelState` (see PHASE_10.md §5.3).

**Priority:** High — blocks the user from inspecting the preview at different zoom levels.

---

## BUG-03 · No loading feedback during file open

**Symptom:** After selecting a file via the open dialog, the UI shows nothing — no
spinner, no progress bar, no indication that work is happening. The tab bar shows the
new tab immediately, but the viewport is black and unresponsive until the entire disk
pipeline finishes (decode → color convert → MIP downsample → cache write). On large
files this takes several seconds with zero user feedback.

A progress bar existed previously but was lost in a refactor.

**Fix direction:** Two parts:

1. **Immediate tab feedback**: As soon as `OpenFile` action fires (before the pipeline
   starts), set `tab.view.loading = true` and `tab.view.progress = 0.0` on the new
   tab. The viewport widget should render a loading overlay (spinner + "Opening…" label)
   whenever `tab.view.loading == true`. This replaces the blank black viewport.

2. **Progress bar restoration**: `PipelineEvent::Progress { done, total }` is already
   emitted by the engine and routed to `App` via `Msg::PipelineEvent`. The controller's
   `handle_pipeline_event` should update `tab.view.progress = done as f32 / total as f32`
   and trigger a redraw. The status bar (or a viewport overlay) reads `tab.view.progress`
   and renders a progress bar. The status bar widget existed and was wired; verify it
   still reads from `tab.view` and is included in the layout.

**Priority:** High — first-run experience; every user hits this on every file open.

---

## BUG-04 · Viewport interactions remain active when modals are open

**Symptom:** As interações com o viewport não cessam quando o modal está aberto. (Interactions with the canvas/viewport do not stop when a modal is open).

**Root cause:** Event bubbling or lack of state check. When modals (like export or filter search) are open, they render as overlays, but the underlying viewport widget still receives and processes mouse/keyboard events (pan, zoom, shortcuts).

**Fix direction:**
- The overlay/modal container needs to capture interactions (e.g., via an opaque `mouse_area` on the backdrop) or the viewport needs a condition to ignore inputs if `app.show_export_dialog` or similar states are active.
- To be addressed later.

**Priority:** Medium — UX issue, but not critically breaking.
