# GPU Kernel Eliding — Overview

## Goal

Two adjacent `BlurKernelGpu` stages in the exec graph (the double-blur test in
`file_ops.rs`) should be compiled into a **single GPU submission** — one
command encoder, one `queue.submit`, one pipeline object — instead of two
separate ones.

Additionally, the codebase currently has six compile errors in
`blur_kernel/gpu.rs` that must be fixed before any new work can be added.

## Terminology

| Term | Meaning |
|---|---|
| **Eliding** | Collapsing N adjacent GPU kernels into a single GPU dispatch sequence |
| **Fusion** | Generating a single WGSL shader module with N entry points, one per pass |
| **Fused runner** | `FusedBlurKernelGpuRunner` — replaces two `BlurKernelGpuRunner`s |

## Why WGSL, Not SPIR-V for Fusion

Static `.spv` files are compiled offline (Slang → SPIR-V via `build.rs`).
Runtime-generated shaders must use WGSL because wgpu can compile WGSL at
runtime but cannot compile SPIR-V at runtime.

Single kernels continue to use the pre-compiled `.spv` from `build.rs`.
Only the fused multi-pass variants are generated as WGSL strings at runtime.

## Why NOT a Single Entry Point for Blur+Blur

Box blur is a **stencil kernel** — every output pixel reads `(2r+1)²`
neighbors.  Fusing two passes into one entry point would require a global
memory barrier between passes, which is NOT available in standard
WebGPU/wgpu (`storageBarrier()` only exists in Chrome's non-standard
extension).

The correct approach: one WGSL module with **two entry points**
(`cs_fused_blur_0`, `cs_fused_blur_1`), recorded as two consecutive compute
passes inside **one** `wgpu::CommandEncoder`, submitted once.

## Architecture After Implementation

```
StateGraph:  FileImage → Blur(r=8) → Blur(r=8) → DisplayCache
                                   ↑
                           FUSED by exec_graph fusion pass

ExecGraph (after fusion pass):
  FileDecoder → ScanLineAccumulator → ColorConvert → NeighborhoodAgg
  → FusedBlurKernelGpu { radii: [8, 8] }
  → Download → TileSink

FusedBlurKernelGpuRunner::process() for each tile:
  alloc src_buf, tmp_buf, dst_buf
  write params_0 uniform (width, height, radius=8, pad)
  write params_1 uniform (width, height, radius=8, pad)
  get/cache compiled shader module (WGSL, cached by radii key)
  encoder.begin_compute_pass → set_pipeline(pass0) → dispatch → end
  encoder.begin_compute_pass → set_pipeline(pass1) → dispatch → end
  copy dst_buf crop to out_buf
  emit Item::Tile
  (batch: submit every BATCH_SIZE tiles)
```

## Binding Layout for Fused N-Blur

```
group(0) binding(0)  uniform  BlurParams { width, height, radius, _pad }  // pass 0
group(0) binding(1)  uniform  BlurParams { width, height, radius, _pad }  // pass 1
...
group(0) binding(N-1) uniform BlurParams  // pass N-1
group(0) binding(N)   storage read        src
group(0) binding(N+1) storage read_write  tmp_0     // between pass 0 and 1
...
group(0) binding(N+N-1) storage read_write  tmp_{N-2}  // between pass N-2 and N-1
group(0) binding(2N)  storage read_write  dst
```

For N=2 passes:
- binding 0: uniform params_0
- binding 1: uniform params_1
- binding 2: storage src (read)
- binding 3: storage tmp (read_write)
- binding 4: storage dst (read_write)

## Implementation Phases

| Phase | File(s) | Description |
|---|---|---|
| P1 | `blur_kernel/gpu.rs` | Fix 6 compile errors |
| P2 | `pixors-shader/src/codegen.rs` (new) | WGSL codegen for N-pass blur |
| P3 | `blur_kernel/fused.rs` (new), `exec/mod.rs` | FusedBlurKernelGpu stage + runner |
| P4 | `exec_graph/fusion.rs`, `exec_graph/mod.rs` | Fusion detection pass |
| P5 | `state_graph/compile.rs` or `exec_graph` | Invoke fusion pass after compile |

Read the phase-specific documents for exact code:
- `GPU_ELIDING_P1_FIXES.md`
- `GPU_ELIDING_P2_CODEGEN.md`
- `GPU_ELIDING_P3_FUSED_KERNEL.md`
- `GPU_ELIDING_P4_FUSION_PASS.md`
