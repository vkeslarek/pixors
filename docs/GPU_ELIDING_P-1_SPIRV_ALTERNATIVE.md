# GPU Eliding — Phase -1: Runtime Slang Composition (libslang)

**Replaces P2 entirely. Modifies P3. P1 + P4 unchanged.**

Read this BEFORE P2/P3 if going Slang-runtime route. P2 (WGSL string codegen)
and this doc are mutually exclusive — pick one.

## Goal

Runtime fusion of N adjacent GPU kernels into a single SPIR-V module + single
`queue.submit`, with **all kernel logic written in Slang**, no enumeration of
combinations, no string-pasted WGSL.

## Strategy: libslang runtime API

`libslang.so` (C++ library, ABI-stable C API via `slang.h`) does in-process
module loading, linking, and SPIR-V emission. `slangc` is just one consumer
of this library. Embed it directly.

Flow at runtime:
1. `IGlobalSession` created once at startup
2. Per chain pattern: load kernel modules (`.slang` source or pre-compiled
   `.slang-module` IR), generate a tiny entry-point module from a template,
   `createCompositeComponentType([modules…])`, `link()`,
   `getEntryPointCode(idx, SLANG_SPIRV)`
3. Cache resulting SPIR-V on disk keyed by `hash(chain pattern)`
4. Feed SPIR-V to wgpu

Cache hit = ~0ms. Cache miss = ~50–100ms one-time per unique chain.

## Why this beats alternatives

| Approach | Verdict |
|---|---|
| Enumerate every chain in build.rs | Combinatorial explosion. Rejected |
| Generate WGSL strings runtime | Bypasses Slang quality. Rejected by user |
| Invoke `slangc` subprocess runtime | Slow (process spawn + disk IO every miss), heavy dep |
| **libslang in-process** | Fast, no subprocess, kernel logic stays in Slang |

## Rust binding choice

System has `~/.local/lib/libslang.so` already (verified). Headers not yet
installed. Three candidates:

1. **`shader-slang/slang-rs`** (upstream Rust bindings, repo
   `github.com/shader-slang/slang-rs`). Status: experimental. Wraps the C++
   COM-style API. Best long-term choice.
2. **`slang-sys`** — raw bindgen on `slang.h`. Available on crates.io
   (third-party). Verify ABI matches installed libslang version.
3. **Custom FFI** — minimal hand-written bindings to the subset of C API we
   use (~6 calls). Lowest external dep, highest maintenance.

**Recommendation:** start with `shader-slang/slang-rs` (path dep on a vendored
checkout), fall back to custom FFI if API mismatches.

Headers needed at build time. Either:
- Install Slang dev package (`slang-dev` if distro has it), or
- Vendor headers from the Slang release tarball into `pixors-shader/vendor/slang/`

## Build-time changes

### Kernel structure (Slang lib)

Each kernel becomes a self-contained Slang module exposing an `IKernel`
implementation, with **no entry point** in the lib file:

`shaders/lib/kernel.slang` (new):
```slang
import lib.neighborhood;

interface IKernel {
    associatedtype Params;
    static float4 sample(Neighborhood nbhd, uint2 center_padded, Params p);
}
```

`shaders/lib/blur_kernel.slang` (new):
```slang
import lib.kernel;
import lib.neighborhood;
import lib.convolution;

public struct BlurParams {
    uint width;
    uint height;
    uint radius;
    uint _pad;
};

public struct BlurKernel : IKernel {
    public typealias Params = BlurParams;

    public static float4 sample(Neighborhood nbhd, uint2 center_padded, BlurParams p) {
        StencilResult r = stencil_sum(nbhd, center_padded, int(p.radius));
        return r.sum / float(r.count);
    }
};
```

### Existing `shaders/blur.slang`

Becomes thin wrapper that uses `BlurKernel`:
```slang
import lib.blur_kernel;

ParameterBlock<BlurParams> params;
StructuredBuffer<uint> src;
RWStructuredBuffer<uint> dst;

[numthreads(8,8,1)]
void cs_blur(uint3 gid : SV_DispatchThreadID) {
    if (gid.x >= params.width || gid.y >= params.height) return;
    Neighborhood nbhd = Neighborhood::bind(src, params.width, params.height, int(params.radius));
    uint2 c = nbhd.center_offset + gid.xy;
    dst[gid.y * params.width + gid.x] = rgba8_pack(BlurKernel::sample(nbhd, c, params));
}
```

Stays compiled by `build.rs` for the **single-kernel** path.

### Optional: pre-compile kernel libs to Slang IR

For faster runtime composition, build.rs can emit `.slang-module` IR per lib
(not SPIR-V — IR is portable, links faster):

```rust
// In pixors-shader/build.rs
let modules = ["lib/neighborhood", "lib/convolution", "lib/kernel",
               "lib/pixel", "lib/blur_kernel"];
for m in modules {
    Command::new("slangc")
        .arg(format!("shaders/{m}.slang"))
        .arg("-target").arg("slang-module")
        .arg("-o").arg(dest.join(format!("{m}.slang-module")))
        .status()?;
}
```

Skip this initially — load `.slang` source at runtime works too, just slower
on first compose.

## Runtime composer module

### File: `pixors-shader/src/composer.rs` (new)

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Description of one step in a fused chain.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct StepDesc {
    pub kernel: KernelKind,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum KernelKind {
    Blur,
    // future: ColorAdjust, Sharpen, ...
}

pub struct ChainComposer {
    session: Arc<slang::Session>, // wrapper over libslang IGlobalSession + ISession
    cache: Mutex<HashMap<u64, Arc<CompiledChain>>>,
    disk_cache_dir: PathBuf,
}

pub struct CompiledChain {
    pub spirv: Vec<u8>,
    pub entry_points: Vec<String>,
    pub binding_layout: BindingLayout,
}

pub struct BindingLayout {
    pub uniforms: Vec<BindingSlot>,
    pub storage: Vec<BindingSlot>,
}

pub struct BindingSlot {
    pub group: u32,
    pub binding: u32,
    pub name: String,
}

impl ChainComposer {
    pub fn new(disk_cache: PathBuf) -> Result<Self, ComposerError> { ... }

    /// Compose a fused chain into a single SPIR-V module with N entry points.
    pub fn compose(&self, steps: &[StepDesc]) -> Result<Arc<CompiledChain>, ComposerError> {
        let key = hash_steps(steps);

        // 1. Memory cache.
        if let Some(c) = self.cache.lock().unwrap().get(&key) {
            return Ok(c.clone());
        }

        // 2. Disk cache.
        let disk_path = self.disk_cache_dir.join(format!("{key:x}.spv"));
        if let Ok(spirv) = std::fs::read(&disk_path) {
            let entry_points = entry_point_names(steps);
            let binding_layout = compute_binding_layout(steps);
            let chain = Arc::new(CompiledChain { spirv, entry_points, binding_layout });
            self.cache.lock().unwrap().insert(key, chain.clone());
            return Ok(chain);
        }

        // 3. Cold compose via libslang.
        let entry_src = generate_entry_point_source(steps);
        let chain = self.session.compose(&entry_src, steps)?;
        let _ = std::fs::write(&disk_path, &chain.spirv);
        let chain = Arc::new(chain);
        self.cache.lock().unwrap().insert(key, chain.clone());
        Ok(chain)
    }
}

/// Generate a small Slang source string with bindings + entry points.
/// All kernel LOGIC stays in lib.*.slang — this only declares bindings and
/// dispatches to lib functions.
fn generate_entry_point_source(steps: &[StepDesc]) -> String {
    let n = steps.len();
    let mut s = String::new();
    s.push_str("import lib.blur_kernel;\n");
    s.push_str("import lib.neighborhood;\n\n");

    // Uniform params per pass.
    for (i, step) in steps.iter().enumerate() {
        match step.kernel {
            KernelKind::Blur => s.push_str(&format!(
                "ParameterBlock<BlurParams> params_{i};\n"
            )),
        }
    }
    // Storage buffers: src, tmp_0..tmp_{n-2}, dst.
    s.push_str("StructuredBuffer<uint> src;\n");
    for k in 0..(n - 1) {
        s.push_str(&format!("RWStructuredBuffer<uint> tmp_{k};\n"));
    }
    s.push_str("RWStructuredBuffer<uint> dst;\n\n");

    // Entry points — each calls into the Slang lib for actual logic.
    for (i, step) in steps.iter().enumerate() {
        let in_buf = if i == 0 { "src".to_string() } else { format!("tmp_{}", i - 1) };
        let out_buf = if i == n - 1 { "dst".to_string() } else { format!("tmp_{i}") };
        match step.kernel {
            KernelKind::Blur => s.push_str(&format!(
                "[numthreads(8,8,1)]\n\
                 void cs_pass_{i}(uint3 gid : SV_DispatchThreadID) {{\n\
                 \tif (gid.x >= params_{i}.width || gid.y >= params_{i}.height) return;\n\
                 \tNeighborhood nbhd = Neighborhood::bind({in_buf}, params_{i}.width, params_{i}.height, int(params_{i}.radius));\n\
                 \tuint2 c = nbhd.center_offset + gid.xy;\n\
                 \t{out_buf}[gid.y * params_{i}.width + gid.x] = rgba8_pack(BlurKernel::sample(nbhd, c, params_{i}));\n\
                 }}\n\n"
            )),
        }
    }
    s
}

fn entry_point_names(steps: &[StepDesc]) -> Vec<String> {
    (0..steps.len()).map(|i| format!("cs_pass_{i}")).collect()
}
```

### Note on the generated source

The generated text is **only**:
- import statements (1–3 lines)
- binding declarations (N + N+1 lines)
- entry point shells that delegate to `BlurKernel::sample(...)` (a few lines each)

**Zero kernel logic** lives in the generated text. All computation is in
`lib/blur_kernel.slang`, `lib/convolution.slang`, `lib/neighborhood.slang` —
type-checked once at build time, written by humans in Slang.

This is the same separation Slang itself enforces: lib code in `lib/*.slang`,
entry-point glue per shader. We just generate the glue at runtime instead of
hand-writing it per chain pattern.

## libslang wrapper layer

### File: `pixors-shader/src/composer/slang_session.rs` (new)

```rust
/// Thin wrapper over libslang's COM-style API.
/// Owns IGlobalSession (process-wide) and one ISession (per ChainComposer).
pub struct Session {
    global: *mut SlangGlobalSession,
    session: *mut SlangSession,
    search_paths: Vec<PathBuf>,
}

impl Session {
    pub fn new(slang_search_paths: &[PathBuf]) -> Result<Self, ComposerError> {
        // 1. createGlobalSession()
        // 2. createSession(target=SPIRV, search_paths)
        // 3. set compile flags: emit-spirv-directly, fvk-use-entrypoint-name
    }

    /// Compile a generated entry-point source + linked kernel modules into
    /// one SPIR-V blob with N entry points.
    pub fn compose(&self, entry_src: &str, steps: &[StepDesc]) -> Result<CompiledChain, ComposerError> {
        // 1. loadModuleFromSourceString(entry_src, "fused_entry")
        // 2. For each unique kernel kind: loadModule("lib/blur_kernel") (cached)
        // 3. findEntryPointByName for each cs_pass_N
        // 4. createCompositeComponentType([entry_module, *kernel_modules, *entry_points])
        // 5. composite.link() → ILinkedComponent
        // 6. For each entry point i: linked.getEntryPointCode(i, target=SPIRV)
        //    → all N entry points end up in ONE SPIR-V if we use
        //    composite.getTargetCode(0, SPIRV) instead.
        // 7. linked.getLayout() → reflection → fill BindingLayout
    }
}
```

### Slang API calls reference

Approximate C API calls (verify exact names/signatures against Slang headers):

```c
slang_createGlobalSession(SLANG_API_VERSION, &global);
SessionDesc desc = { .targets = {{.format = SLANG_SPIRV, .profile = SLANG_PROFILE_GLSL_460}} };
global->createSession(&desc, &session);

IModule* mod_entry  = session->loadModuleFromSourceString("fused_entry", "/virt/fused.slang", entry_src, &diags);
IModule* mod_blur   = session->loadModule("lib.blur_kernel", &diags);

IEntryPoint* eps[N];
for (i=0; i<N; i++) mod_entry->findEntryPointByName(("cs_pass_" + i).c_str(), &eps[i]);

IComponentType* parts[] = { mod_entry, mod_blur, eps[0], ..., eps[N-1] };
IComponentType* composite;
session->createCompositeComponentType(parts, N+2, &composite, &diags);

IComponentType* linked;
composite->link(&linked, &diags);

ISlangBlob* spirv_blob;
linked->getTargetCode(0, &spirv_blob, &diags);  // single SPIR-V with all entry points

ProgramLayout* layout = linked->getLayout();
// walk layout for binding reflection
```

## Cache strategy

```
~/.cache/pixors/shader-cache/
  <hash>.spv         — composed SPIR-V
  <hash>.layout      — serialized BindingLayout (json or bincode)
  manifest.json      — version + slang lib hash for invalidation
```

Invalidation key = `hash(steps) ⊕ hash(slang lib content) ⊕ slang_version`.
If lib source changes (rebuild), all chain hashes invalidate.

## Integration changes

### Replaces P2 entirely

Delete `pixors-shader/src/codegen.rs` from the plan (WGSL string generator).
Replace with:
- `pixors-shader/src/composer.rs` (this file's design)
- `pixors-shader/src/composer/slang_session.rs` (libslang wrapper)
- `pixors-shader/src/composer/cache.rs` (disk cache)

Add to `pixors-shader/src/lib.rs`:
```rust
pub mod composer;
```

### Modifies P3 (`blur_kernel/fused.rs`)

`FusedBlurKernelGpuRunner::get_or_build_pipelines` changes from generating WGSL
to:

```rust
fn get_or_build_pipelines(&mut self, device: &wgpu::Device, composer: &ChainComposer)
    -> Result<&FusedPipelineSet, Error>
{
    let key = hash_radii(&self.radii);
    if self.pipeline_cache.contains_key(&key) {
        return Ok(&self.pipeline_cache[&key]);
    }

    let steps: Vec<StepDesc> = self.radii.iter()
        .map(|_| StepDesc { kernel: KernelKind::Blur })
        .collect();
    let chain = composer.compose(&steps)?;

    // Build BGL from chain.binding_layout (reflection-driven, not hardcoded).
    let bgl = build_bgl_from_layout(device, &chain.binding_layout);

    // Single shader module from one SPIR-V containing all entry points.
    let shader_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fused_chain_spirv"),
        source: wgpu::ShaderSource::SpirV(spirv_to_words(&chain.spirv)),
    });
    let layout = device.create_pipeline_layout(...);
    let pipelines: Vec<_> = chain.entry_points.iter()
        .map(|ep| Arc::new(device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            entry_point: Some(ep.as_str()),
            module: &shader_mod,
            layout: Some(&layout),
            ...
        })))
        .collect();

    self.pipeline_cache.insert(key, FusedPipelineSet { bgl, pipelines, num_passes: self.radii.len() });
    Ok(&self.pipeline_cache[&key])
}
```

### `ChainComposer` lifetime

Owned by `GpuContext` (singleton, init lazy alongside scheduler):
```rust
// pixors-engine/src/gpu/context.rs
pub struct GpuContext {
    scheduler: Arc<Scheduler>,
    composer: Arc<ChainComposer>,  // new
}
```

Runner accesses via `ctx.composer()`.

## Reflection-driven BGL

libslang reflection exposes for each parameter:
- name (e.g., `params_0`, `src`, `tmp_0`, `dst`)
- binding index
- group index
- resource type (uniform / storage read / storage read_write)

Build BGL from this instead of hardcoding indices. Future kernel mixes
(e.g., `Blur → ColorAdjust → Blur`) work with zero changes to runner code —
binding layout falls out of Slang reflection.

## Cargo.toml changes

`pixors-shader/Cargo.toml`:
```toml
[dependencies]
wgpu = { version = "22.1", default-features = false, features = ["spirv"] }
slang = { git = "https://github.com/shader-slang/slang-rs", rev = "..." }
# OR
# slang-sys = "0.x"

[build-dependencies]
# nothing new — build.rs still uses slangc CLI for single-kernel path
```

`build.rs`: emit `cargo:rustc-link-search=$HOME/.local/lib` so linker finds
`libslang.so`. Or set up `pkg-config` if Slang ships a `.pc` file.

## Risks / open questions

1. **slang-rs maturity**: upstream bindings may have rough edges. May need
   to fork or use custom FFI. Spike before committing.

2. **ABI stability**: Slang's C API (`slang.h`) is documented stable, but
   `~/.local/lib/libslang.so.0.2026.8` is a recent build — verify on other
   machines (CI, distribution).

3. **Cold compose latency**: 50–100ms per unique chain pattern. Pre-warm
   cache on app startup for known patterns (`[Blur]`, `[Blur, Blur]`)?

4. **Distribution**: shipping `libslang.so` with desktop binary means ~30MB
   bundled. Acceptable? Or static link a Slang build?

5. **Headers**: `~/.local/include/slang*` empty. Need to install Slang
   dev package or vendor headers.

6. **Reflection accuracy**: confirm libslang reflection reports the same
   binding indices that the SPIR-V actually uses. There were historic issues
   with implicit bindings — `-fvk-use-entrypoint-name` flag must be passed.

## Validation plan

1. **Spike** (1–2 days): minimal Rust binary that calls libslang to compose
   a `[Blur, Blur]` chain and emits SPIR-V. Verify output runs in wgpu and
   matches CPU reference within 1 LSB.

2. **Headers + linking**: confirm `cargo build` finds libslang on dev
   machine, then on CI Linux runner.

3. **Disk cache invalidation**: edit `lib/blur_kernel.slang`, rebuild,
   confirm cached SPIR-V regenerates (manifest hash detects change).

4. **End-to-end**: full `file_ops.rs` flow with double-blur, confirm one
   `queue.submit` per tile batch, output pixels match single-kernel path
   applied twice.

## Phase order with this alternative

```
P1   fix compile errors                          (unchanged)
P-1  libslang composer + wrapper                 (this doc)  ← replaces P2
P3   FusedBlurKernelGpu uses composer            (modified per §"Modifies P3")
P4   exec_graph fusion pass                      (unchanged)
```
