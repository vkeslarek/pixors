# Roadmap — Future Improvements

Ideas captured during Phase 9 planning. Not scheduled, not scoped. Revisit after Phase 9 ships.

---

## Lock zoom during Preview

When a Preview is active (user is scrubbing a blur radius slider), disallow zoom
changes. The preview pipeline operates on a fixed MIP level. Changing zoom would
invalidate the preview's visible tiles mid-apply.

**Current behavior (Phase 9):** Zoom cancels the preview and starts a new one at
the new MIP level. This works but causes a brief flicker as old tiles are discarded
and new ones recomputed.

**Ideal behavior:** The zoom gesture is blocked while `PreviewService` has an active
preview. The frontend disables the zoom gesture (wheel/pinch) and the backend
rejects `ViewportUpdate` commands while preview is `Running`.

**Complexity:** Requires coordination between ViewportService and PreviewService.
Frontend needs a `preview_active: bool` state to disable the zoom gesture. Backend
needs a guard in `ViewportCommand::Update` handler.

**Priority:** Low. Cancel-on-zoom is good enough for Phase 9.
