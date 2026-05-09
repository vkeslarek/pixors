# Refactor Roadmap

---

## P4 — Action Layer Hardening

### A6 — Async file dialog
Controller blocks UI thread on `rfd::FileDialog::pick_file()`. Need to change `update()` return type from `()` to `iced::Task<Msg>` and use `rfd::AsyncFileDialog` with `Task::perform()`.

---

## P5 — Dedup Helpers

### D3 — consolidate_tiles helper
`to_neighborhood.rs` duplicates tile consolidation logic. Extract to `pixors-engine/src/data_transform/consolidate.rs`.

### D4 — Shared tile-grid assembler
`png_encoder_v2.rs` and `tiff_encoder.rs` have identical tile assembly code. Extract to `pixors-image/src/sink/mod.rs`.

### D6 — Color LUT
Two 16-arm `match` blocks in `color.rs` for kernel dispatch. Replace with static `KERNEL_TABLE` lookup.

---

## P6 — Robustness

### R2 — merge_inputs: scoped threads
`merge_inputs()` spawns detached threads. Use `std::thread::scope` so they're joined before the function returns.

### R4 — chain: blocking recv
Chain runner uses `recv_timeout(100ms)` polling loop. Replace with `recv()` and rely on `AtomicBool` cancellation check per-item.

---

## Deferred

### MCP Server
`pixors-mcp/src/index.ts` calls `http://127.0.0.1:8080` which does not exist. Tab is now Send+Sync — unblocked. Options:
1. `pixors-server` crate (axum + WS)
2. Stdio + JSON-RPC
3. `napi-rs` FFI

### Test floor
No integration tests yet. Priority: dispatcher, compose, color round-trip, codec round-trip.
