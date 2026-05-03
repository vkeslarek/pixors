# GPU Eliding — Implementation Checklist

Work through phases in order. Each phase must compile before starting the next.

## Phase 1 — Fix Compile Errors

File: `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs`

- [ ] Delete the dead `impl GpuKernel for BlurKernel { ... }` block (lines ~18–34,
      the block that references `GpuKernel`, `KernelSig`, `BLUR_SIG`)
- [ ] Add `const BATCH_SIZE: usize = 16;` after the imports

File: `pixors-engine/src/pipeline/exec/tile_sink.rs`

- [ ] Remove `Mutex` from `use std::sync::{Arc, Mutex, OnceLock}` → `{Arc, OnceLock}`
- [ ] Remove `use crate::container::Tile;`

**Checkpoint:** `cargo check -p pixors-engine` → no errors

---

## Phase 2 — WGSL Codegen

New file: `pixors-shader/src/codegen.rs`

- [ ] Implement `pub fn gen_fused_blur(radii: &[u32]) -> FusedBlurShader`
      (full code in `GPU_ELIDING_P2_CODEGEN.md`)
- [ ] Register: add `pub mod codegen;` to `pixors-shader/src/lib.rs`

File: `pixors-shader/src/scheduler.rs`

- [ ] Add `pub fn build_fused_blur_bgl(device: &wgpu::Device, n: usize) -> Arc<wgpu::BindGroupLayout>`
      (full code in `GPU_ELIDING_P2_CODEGEN.md`)

**Checkpoint:** `cargo check -p pixors-shader` → no errors

Optional: write the codegen unit test from P2 doc and run it.

---

## Phase 3 — FusedBlurKernelGpu Stage + Runner

New file: `pixors-engine/src/pipeline/exec/blur_kernel/fused.rs`

- [ ] Implement `FusedBlurKernelGpu` (Stage) and `FusedBlurKernelGpuRunner` (OperationRunner)
      (full code in `GPU_ELIDING_P3_FUSED_KERNEL.md`)

File: `pixors-engine/src/pipeline/exec/blur_kernel/mod.rs`

- [ ] Add `pub mod fused;`
- [ ] Add `pub use fused::{FusedBlurKernelGpu, FusedBlurKernelGpuRunner};`

File: `pixors-engine/src/pipeline/exec/mod.rs`

- [ ] Add `FusedBlurKernelGpu` to the `pub use blur_kernel::{...}` line
- [ ] Add `FusedBlurKernelGpu,` to the `ExecNode` enum

**Checkpoint:** `cargo check -p pixors-engine` → no errors

---

## Phase 4 — Fusion Detection Pass

File: `pixors-engine/src/pipeline/exec_graph/fusion.rs`

- [ ] Replace entire file with new implementation (full code in `GPU_ELIDING_P4_FUSION_PASS.md`)
- [ ] `pub fn fuse_gpu_kernels(graph: &ExecGraph) -> ExecGraph`

File: `pixors-engine/src/pipeline/exec_graph/mod.rs`

- [ ] Add `pub mod fusion;`

File: `pixors-engine/src/pipeline/state_graph/compile.rs`

- [ ] Add fusion pass call at end of `compile()`, before `Ok(exec)`:
      ```rust
      let exec = crate::pipeline::exec_graph::fusion::fuse_gpu_kernels(&exec);
      ```

**Checkpoint:** `cargo check --workspace` → no errors

---

## Phase 5 — Test

- [ ] Write `double_blur_fuses` test (in `GPU_ELIDING_P4_FUSION_PASS.md`)
- [ ] `cargo test -p pixors-engine -- double_blur_fuses`
- [ ] Open the desktop app, load an image (double-blur is hardcoded in `file_ops.rs`)
- [ ] Verify image renders correctly (blurred, not black)
- [ ] Check logs for `fused_blur_gpu: total N tiles` (not `blur_kernel_gpu`)

---

## Files Changed Summary

| File | Change |
|---|---|
| `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs` | Delete dead impl block, add BATCH_SIZE |
| `pixors-engine/src/pipeline/exec/tile_sink.rs` | Remove unused imports |
| `pixors-shader/src/codegen.rs` | **NEW** — WGSL codegen |
| `pixors-shader/src/scheduler.rs` | Add `build_fused_blur_bgl` |
| `pixors-shader/src/lib.rs` | Add `pub mod codegen` |
| `pixors-engine/src/pipeline/exec/blur_kernel/fused.rs` | **NEW** — fused stage |
| `pixors-engine/src/pipeline/exec/blur_kernel/mod.rs` | Add `pub mod fused` + re-export |
| `pixors-engine/src/pipeline/exec/mod.rs` | Add `FusedBlurKernelGpu` to enum |
| `pixors-engine/src/pipeline/exec_graph/fusion.rs` | Replace with chain-detection pass |
| `pixors-engine/src/pipeline/exec_graph/mod.rs` | Add `pub mod fusion` |
| `pixors-engine/src/pipeline/state_graph/compile.rs` | Call fusion pass after compile |
| `pixors-shader/src/kernel.rs` | Remove `fusable_body` method |

## Key Invariants to Maintain

1. `BlurKernelGpu` stays in the `ExecNode` enum and `blur_kernel/gpu.rs` — it's
   still needed for single-blur pipelines.

2. `FusedBlurKernelGpu` only appears AFTER the fusion pass — never emitted by
   `state::Blur::expand()`.

3. The fused runner MUST handle N=1 correctly (though the pass won't create
   N=1 fused nodes in practice).

4. Neighborhood radius for fused blur: use max(radii) as the padded region.
   Each pass reads from a padded neighborhood, so the src buffer must be padded
   by the total maximum radius needed.

5. Binding indices must match exactly between `codegen.rs` and
   `fused.rs`. The BGL from `build_fused_blur_bgl(device, n)` must match the
   `@group(0) @binding(k)` declarations in the generated WGSL.
