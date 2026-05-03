# GPU Eliding — Phase 1: Fix Compile Errors

`cargo check -p pixors-engine` currently fails with 6 errors. Fix these first.

## Error List

```
error[E0405]: cannot find trait `GpuKernel` in this scope
error[E0425]: cannot find type `BlurKernel` in this scope        (impl target)
error[E0425]: cannot find type `KernelSig` in this scope
error[E0425]: cannot find value `BLUR_SIG` in this scope
error[E0425]: cannot find value `BATCH_SIZE` in this scope       (line ~308)
error[E0425]: cannot find value `BATCH_SIZE` in this scope       (line ~320)
```

All 6 are in `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs`.

## File: `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs`

### Problem

Lines 18–34 contain a dead `impl GpuKernel for BlurKernel` block:

```rust
impl GpuKernel for BlurKernel {        // GpuKernel never imported; BlurKernel is CPU type
    fn sig(&self) -> &KernelSig {       // KernelSig never imported
        &BLUR_SIG                        // BLUR_SIG never defined
    }
    fn write_params(&self, dst: &mut [u8]) { ... }
}
```

This block is **never called** — `BlurKernelGpuRunner::process()` creates its
own pipeline manually and does not use the `GpuKernel` trait.  It was leftover
scaffolding that was never wired up.

`BATCH_SIZE` is used at lines ~308 and ~320 but never defined in the file.

### Fix

**Step 1.** Delete lines 18–34 (the entire `impl GpuKernel for BlurKernel`
block including the `#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)] struct
BlurParams` definition that references it — wait, `BlurParams` is also used at
line ~149 in the runner, so only delete the trait impl, not the struct).

Specifically, delete **only** this block (verify line numbers with your editor):

```rust
impl GpuKernel for BlurKernel {
    fn sig(&self) -> &KernelSig {
        &BLUR_SIG
    }

    fn write_params(&self, dst: &mut [u8]) {
        let w = 0u32;
        let h = 0u32;
        let params = BlurParams {
            width: w,
            height: h,
            radius: self.radius,
            _pad: 0,
        };
        dst[..16].copy_from_slice(bytemuck::bytes_of(&params));
    }
}
```

**Step 2.** After the `use crate::debug_stopwatch;` import line, add:

```rust
const BATCH_SIZE: usize = 16;
```

**Step 3.** The `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct
BlurKernelGpu { pub radius: u32 }` block needs `Serialize`/`Deserialize`
derives. Verify the existing derives include them (they do, based on the file
content shown).

### Result

After these changes, `cargo check -p pixors-engine` should pass with only the
2 pre-existing warnings (unused imports in `tile_sink.rs`).

## File: `pixors-engine/src/pipeline/exec/tile_sink.rs`

Fix 2 warnings (these are `#[warn]` not `#[deny]` so they don't block
compilation, but clean them up):

**Line 1:** Remove `Mutex` from the use statement:
```rust
// Before:
use std::sync::{Arc, Mutex, OnceLock};
// After:
use std::sync::{Arc, OnceLock};
```

**Line 6:** Remove unused import:
```rust
// Before:
use crate::container::Tile;
// After:
// (delete this line)
```

## Verify

```bash
cargo check -p pixors-engine 2>&1 | grep "^error"
# Should produce: (no output)
```

Then:
```bash
cargo check --workspace 2>&1 | grep "^error"
```
