# Known Bugs

---

## BUG-01 · Viewport interactions remain active when modals are open

**Symptom:** The viewport still receives and processes mouse/keyboard events (pan, zoom,
shortcuts) when modals like export or filter search are open.

**Priority:** Medium — to be addressed later.

---

## BUG-02 · Viewport renders darker than source image

**Symptom:** The image displayed in the viewport is noticeably darker than the source
image. Export (PNG/TIFF/WebP) produces correct gamma and color — the bug is display-only.

**Likely cause:** Gamma or color space mismatch in the viewport rendering pipeline
(`ViewportSink` → GPU texture → Iced compositor). The working color space is ACEScg
(linear), the display format is sRGB (gamma-encoded). The final copy to the Iced
texture may be skipping the sRGB gamma encode or the compositor is applying an
extra gamma correction.

**Priority:** High — affects every image viewed in the editor.

