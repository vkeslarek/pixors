# Known Bugs

---

## BUG-01 · Blur preview drops random tiles — ✅ FIXED

**Symptom:** During fast blur preview (slider drag), some tiles render blurred correctly
while others disappear entirely, leaving black or transparent gaps across the viewport.

**Root cause:** `TileCache::tiles_at_mip` with `generation > 0` returned only overlay
tiles. Base tiles for positions not yet written by the preview pipeline were omitted,
causing those positions to show black.

**Fix (2026-05-11):** `tiles_at_mip` now returns overlay tiles PLUS base-tier fallbacks
for any position the overlay hasn't written yet. Overlay tiles win when present; base
fills the gaps. Change in `pixors-desktop/src/viewport/tile_cache.rs`.

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
