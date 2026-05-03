# Pixors — GPU Pipeline & Viewport Refactor Plan

> Target reader: another AI agent or engineer with full repo access. This plan is prescriptive: file paths, signatures, ordered steps. Do not deviate without writing a follow-up plan.

## 0. Goals

1. **Viewport rewrite (`pixors-desktop`)**: replace the single CPU-backed RGBA8 mirror buffer with a tiled GPU texture that owns a MIP chain. Tiles are written to the texture directly from engine tile buffers. Sampler does anisotropic + trilinear filtering for zoom-out quality.
2. **GPU stage refactor (`pixors-engine`)**: pull all wgpu resource management (buffer creation, bind groups, pipeline creation, encoder ownership, dispatch loop) out of individual `Stage` impls. Stages become pure declarations — `KernelSig` (resource description) + WGSL/Slang body. A new `gpu::scheduler` walks the exec graph, allocates resources, fuses adjacent compatible kernels into a single shader+dispatch (kernel elision).
3. **Adopt Slang** as the shader authoring language. Slang compiles to WGSL via `slang-rs` (or `slangc` invoked from `build.rs`). Stages reference Slang modules. Fusion happens at WGSL level after Slang compilation, OR (preferred long-term) via Slang generic specialization.

## 1. Current state recap (already verified)

- `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs` is the canonical "bad" stage: it calls `ctx.device.create_buffer`, `create_bind_group`, owns the `CommandEncoder`, batches submits, manages `keepalive: Vec<Arc<wgpu::Buffer>>` itself. Anything new GPU stage written today copies this pattern.
- `pixors-engine/src/pipeline/exec/upload.rs` does its own `create_buffer_init`. `display_sink.rs` only handles CPU; if we want GPU tile commit we need a new sink.
- `pixors-engine/src/gpu/kernels/blur.rs` holds the WGSL shader + `BlurPipeline { pipeline, bgl }` cached in a `OnceLock`. This is the only abstraction layer between stage and raw wgpu — too thin.
- `pixors-desktop/src/engine.rs` (613 lines) is the iced shader widget. `EnginePipeline` recreates the entire render pipeline from scratch on every texture-dim change (line 305–340), uses no MIPs (`mip_level_count: 1`), uploads the whole image as one CPU `Vec<u8>` via `init_buffer(w, h)` shared with `display_sink`, and the fragment shader does manual point sampling with `if` clamping (lines 600–612). No tiles.
- `pixors-engine/src/container/tile.rs::Tile` already has `coord: TileCoord` (with `tx, ty, px, py`) — we use that to index into the destination texture directly.
- `Item::Tile` carries `Buffer::Cpu(Arc<Vec<u8>>)` or `Buffer::Gpu(GpuBuffer)`. We must support both endpoints in the new viewport sink.

## 2. Phasing

The work is split into 4 phases. Each phase compiles, passes `cargo test --workspace` and `cargo clippy --workspace`, and produces a runnable app. Do **not** start a phase before the previous one is green and committed.

| Phase | Scope | Branch suggestion |
|---|---|---|
| P1 | New viewport: tiled texture + MIPs (no engine changes beyond a new sink) | `feature/viewport-tiles` |
| P2 | Engine GPU resource ownership: `gpu::pool`, `gpu::scheduler`, `KernelSig`, port `BlurKernelGpu` to declarative form | `feature/gpu-kernel-decl` |
| P3 | Kernel fusion (point-kernel chains first) | `feature/gpu-fusion` |
| P4 | Migrate kernel sources to Slang, add build.rs codegen | `feature/slang-shaders` |

Each phase must end with a PR-ready commit set. Do not bundle phases.

---

## Phase 1 — Viewport: tiled texture + MIPs

### 1.1 New module layout (`pixors-desktop/src/viewport/`)

Replace the placeholder `mod.rs` (currently 1 line) and split `engine.rs` into proper modules.

```
pixors-desktop/src/viewport/
├── mod.rs              # public surface
├── camera.rs           # KEEP existing (pixors-desktop/src/viewport/camera.rs)
├── tiled_texture.rs    # NEW — owns wgpu::Texture, MIP chain, tile upload
├── mip_builder.rs      # NEW — compute pass that downsamples into MIP levels
├── pipeline.rs         # NEW — RenderPipeline wrapping the existing fragment shader
├── program.rs          # NEW — iced::widget::shader::Program impl (was engine.rs)
└── tile_sink.rs        # NEW — engine sink that pushes tiles into TiledTexture
```

Delete `pixors-desktop/src/engine.rs` after the split. Update `pixors-desktop/src/main.rs` line 2 (`mod engine;` → remove) and `pixors-desktop/src/ui/app.rs` line 194 to use `viewport::tile_sink::install(...)` instead of `display_sink::init_buffer`.

### 1.2 `viewport::tiled_texture::TiledTexture`

Struct definition:

```rust
pub struct TiledTexture {
    texture: wgpu::Texture,
    full_view: wgpu::TextureView,        // all mips
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    tile_size: u32,                      // typically 256 or 512
    mip_count: u32,
    /// Tracks which tiles have been written (for partial/lazy MIP regen).
    dirty_tiles: HashSet<(u32, u32)>,    // (tx, ty) at mip 0
    /// Tiles whose full mip chain has been (re)generated since last query.
    mip_dirty: bool,
}

impl TiledTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, tile_size: u32) -> Self;

    /// Upload one tile from a CPU byte slice. `bytes` is `tile_w * tile_h * 4` RGBA8.
    pub fn write_tile_cpu(
        &mut self,
        queue: &wgpu::Queue,
        coord: TileCoord,
        bytes: &[u8],
    );

    /// Upload one tile by GPU-to-GPU copy from an existing wgpu::Buffer.
    /// Used by the engine's `tile_sink` when `Buffer::Gpu` is present.
    pub fn write_tile_gpu(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        coord: TileCoord,
        src_buffer: &wgpu::Buffer,
        src_bpr: u32,           // bytes per row in src
    );

    /// Regenerate MIP levels for the tiles in `dirty_tiles`, then clear.
    pub fn regenerate_mips(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        builder: &MipBuilder,
    );

    pub fn view(&self) -> &wgpu::TextureView { &self.full_view }
    pub fn sampler(&self) -> &wgpu::Sampler { &self.sampler }
    pub fn dims(&self) -> (u32, u32) { (self.width, self.height) }
}
```

Key implementation rules:

- Texture descriptor:
  ```rust
  wgpu::TextureDescriptor {
      label: Some("viewport_tiled_texture"),
      size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
      mip_level_count: ((width.max(height) as f32).log2().floor() as u32) + 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8UnormSrgb,
      usage: wgpu::TextureUsages::TEXTURE_BINDING
           | wgpu::TextureUsages::COPY_DST
           | wgpu::TextureUsages::STORAGE_BINDING       // for mip compute
           | wgpu::TextureUsages::RENDER_ATTACHMENT,    // alt mip path
      view_formats: &[],
  }
  ```
  `STORAGE_BINDING` requires the wgpu device to be requested with `Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` not present in current `Limits::downlevel_defaults` — see §1.5.

- Sampler:
  ```rust
  wgpu::SamplerDescriptor {
      address_mode_u: ClampToEdge,
      address_mode_v: ClampToEdge,
      address_mode_w: ClampToEdge,
      mag_filter: Linear,
      min_filter: Linear,
      mipmap_filter: Linear,
      lod_min_clamp: 0.0,
      lod_max_clamp: 32.0,
      anisotropy_clamp: 4,    // requires Features::ANISOTROPY when > 1
      ..Default::default()
  }
  ```
  Anisotropy requires no special feature flag in wgpu 0.20+ but check actual wgpu version in `pixors-desktop/Cargo.toml`. If the iced-bundled wgpu version refuses `anisotropy_clamp > 1`, fall back to `1`.

- `write_tile_cpu` rules:
  - `bytes_per_row` MUST be aligned to `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT` (256). For tiles whose `tile_w * 4` is a multiple of 256 (e.g. 256-wide tile), no padding needed. For edge tiles where `coord.width < tile_size`, pad row stride.
  - Insert `(coord.tx, coord.ty)` into `dirty_tiles` and set `mip_dirty = true`.

- `write_tile_gpu` uses `encoder.copy_buffer_to_texture` with the source buffer's row pitch (engine guarantees `tile_w * 4` rows packed for tile buffers, but verify). Source row pitch alignment same constraint applies.

### 1.3 `viewport::mip_builder::MipBuilder`

Compute-shader-based MIP generation. One pipeline per source-mip-level (or one parametric pipeline reading `src_mip` uniform). WGSL pseudocode:

```wgsl
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var samp: sampler;

@compute @workgroup_size(8, 8, 1)
fn cs_downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dst_dim = textureDimensions(dst);
    if (gid.x >= dst_dim.x || gid.y >= dst_dim.y) { return; }
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dst_dim);
    let c = textureSampleLevel(src, samp, uv, 0.0);
    textureStore(dst, vec2<i32>(gid.xy), c);
}
```

Driver code creates per-level `TextureView` with `base_mip_level: i, mip_level_count: Some(1)`, binds level `i` as src and level `i+1` as dst, dispatches `(dst_w/8, dst_h/8, 1)`. Loop from `i = 0..mip_count - 1`.

Optimization later: only regenerate mip chains intersecting `dirty_tiles`. For P1, regenerate the whole chain whenever `mip_dirty`. The compute is < 1ms for 4K images.

### 1.4 `viewport::pipeline::ViewportPipeline`

Replaces `engine.rs::EnginePipeline`. Responsibilities:

- Owns the render pipeline (uses `format` from `iced::widget::shader::Pipeline::new`).
- Owns the camera uniform buffer.
- Owns the bind group layout (bindings 0=uniform, 1=texture, 2=sampler).
- Holds an `Option<Arc<Mutex<TiledTexture>>>`. Bind group is rebuilt **only** when texture identity changes (i.e. user opens a new image), NOT on every dim change. Texture object stays alive across re-draws.
- DOES NOT recreate the render pipeline on texture-dim change. That bug must die — pipeline depends only on bind group layout and target format, both invariant.

Updated fragment shader (replaces lines 578–613 of `engine.rs`):

```wgsl
struct Camera {
    vp_w: f32, vp_h: f32,
    img_w: f32, img_h: f32,
    pan_x: f32, pan_y: f32,
    zoom: f32, _pad: f32,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var t: texture_2d<f32>;
@group(0) @binding(2) var s: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var o: VsOut;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // map screen pixel → image pixel via camera
    let screen = in.uv * vec2<f32>(cam.vp_w, cam.vp_h);
    let img_xy = screen / cam.zoom + vec2<f32>(cam.pan_x, cam.pan_y);
    if (img_xy.x < 0.0 || img_xy.y < 0.0
        || img_xy.x >= cam.img_w || img_xy.y >= cam.img_h) {
        return vec4<f32>(0.067, 0.067, 0.075, 1.0);
    }
    let uv = img_xy / vec2<f32>(cam.img_w, cam.img_h);
    // textureSample picks LOD automatically from screen-space derivatives.
    return textureSample(t, s, uv);
}
```

The auto-LOD via `textureSample` (not `textureSampleLevel`) is what makes MIPs do their job; the GPU computes `ddx/ddy` of `uv` itself.

### 1.5 wgpu device requirements

`pixors-engine/src/gpu/context.rs::init_inner` line 67–73 currently requests `required_features: wgpu::Features::empty()`. **Compute writing to storage textures requires** `wgpu::Features::TEXTURE_BINDING_ARRAY` is NOT what we need; we need write access to a storage texture which works under `downlevel_defaults` for `rgba8unorm` only if the adapter advertises `TEXTURE_FORMAT_RGBA8_UNORM_STORAGE`. Add:

```rust
required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
required_limits: wgpu::Limits::default(),  // not downlevel
```

If a target adapter rejects `default()` limits, fall back to a render-pipeline-based MIP path (draw a fullscreen triangle into each mip level as a render attachment). For Phase 1 keep one path; pick the storage-texture one if the dev machine supports it (NVIDIA/AMD discrete: yes; Intel iGPU: usually yes; old MoltenVK: no).

NOTE: the desktop wgpu instance is the one bundled by `iced` (re-exported as `iced::wgpu`). The engine's `GpuContext` is a **different** device. We cannot share textures across devices. Phase 1 keeps the viewport texture on the **iced wgpu device** and continues to download tiles from the engine device to CPU before re-uploading. Cross-device sharing is Phase 2.5 / Phase 3 — see §3.5.

### 1.6 `viewport::tile_sink::TileSink` (engine-side stage)

New ExecNode that replaces `DisplaySink` for the desktop. Lives in `pixors-engine/src/pipeline/exec/tile_sink.rs`:

```rust
pub struct TileSink;

// callback registered by desktop at startup:
// fn(coord: TileCoord, rgba8: &[u8])
type TileCommitFn = Box<dyn Fn(TileCoord, &[u8]) + Send + Sync>;

static TILE_SINK: OnceLock<Arc<TileCommitFn>> = OnceLock::new();
pub fn install_tile_sink(f: TileCommitFn);
```

Runner:
- Receives `Item::Tile`. If `Buffer::Cpu`, call commit immediately.
- If `Buffer::Gpu`, do the existing readback (re-use logic from `Download` stage), then call commit. Phase 2.5 will replace this with cross-device handoff.

Add `TileSink` to `ExecNode` enum in `pixors-engine/src/pipeline/exec/mod.rs` line 68. Bump `pub use` block accordingly.

### 1.7 `pixors-desktop/src/ui/app.rs` integration

Line 194 currently:
```rust
let gpu = display_sink::init_buffer(w, h);
{ let mut state = gpu.lock().unwrap();
  state.pixels.copy_from_slice(&rgba); state.dirty = true; }
self.gpu_buffer = Some(gpu);
```

Replace with:
```rust
let tex = Arc::new(Mutex::new(TiledTexture::new(&device, w, h, 256)));
self.tiled_texture = Some(tex.clone());

// install engine sink callback so subsequent operations land here
let cb_tex = tex.clone();
pixors_engine::pipeline::exec::tile_sink::install_tile_sink(Box::new(
    move |coord, rgba| {
        let mut t = cb_tex.lock().unwrap();
        t.write_tile_cpu(&queue, coord, rgba);
        // mip regen runs in viewport::pipeline::prepare()
    },
));

// initial blit: split rgba into tiles and write_tile_cpu for each
for ty in 0..h.div_ceil(256) {
    for tx in 0..w.div_ceil(256) {
        let coord = TileCoord::new(tx, ty, 256, w, h);
        let bytes = extract_tile_bytes(&rgba, w, &coord);
        tex.lock().unwrap().write_tile_cpu(&queue, coord, &bytes);
    }
}
```

Problem: `device` and `queue` are not available at `open_file_dialog` time — those live inside iced's `shader::Pipeline`. Solution: stash the `Arc<Mutex<TiledTexture>>` in `App`, defer writes via a queue (`Vec<(TileCoord, Vec<u8>)>` drained in `ViewportPipeline::prepare`). The first `prepare` call after a file load drains the queue and writes tiles + regenerates mips.

Concrete shape:

```rust
pub struct PendingTileWrites {
    pub queue: Mutex<Vec<(TileCoord, Vec<u8>)>>,
    pub realloc: Mutex<Option<(u32, u32)>>,  // Some when image dims changed
}
```

Share `Arc<PendingTileWrites>` between `App` and `ViewportPipeline`. `App` pushes; `prepare` drains.

### 1.8 Kill paths

Remove or deprecate:
- `pixors-engine/src/pipeline/exec/display_sink.rs::init_buffer`, `display_buffer`, `GpuBufferState`, `DisplaySink`. All call sites move to `tile_sink`.
- The whole CPU-mirror path in `pixors-desktop/src/engine.rs` (file deleted entirely after extraction).

### 1.9 P1 acceptance checklist

- [ ] Open a 4K image: visible immediately, not as one big upload — tile-by-tile fill (acceptable to be near-instant for now since current path is single-shot).
- [ ] Zoom out to fit a 8K image in 800px viewport: NO pixel shimmer / aliasing (MIPs working).
- [ ] Zoom in to 16x: linear filter visible (already the case, but verify nothing regressed).
- [ ] `cargo clippy --workspace` clean.
- [ ] `cargo test --workspace` green.

---

## Phase 2 — Engine GPU Kernel Declarative API

### 2.1 New module layout (`pixors-engine/src/gpu/`)

```
pixors-engine/src/gpu/
├── mod.rs
├── context.rs           # KEEP (already singleton)
├── buffer.rs            # KEEP
├── pool.rs              # NEW — buffer pool keyed by (size_class, usage)
├── kernel.rs            # NEW — KernelSig, KernelBody, ResourceDecl, ParamDecl
├── codegen.rs           # NEW — assembles WGSL source from kernel sigs + bodies
├── scheduler.rs         # NEW — drives execution: alloc, bind, dispatch, fuse
└── kernels/
    ├── mod.rs
    └── blur.wgsl        # bodies move out of .rs into pure .wgsl files
```

The existing `kernels/blur.rs` (a custom pipeline cache) is deleted; replaced by `gpu::scheduler` which caches pipelines keyed by `KernelSig` hash.

### 2.2 `gpu::pool::BufferPool`

```rust
pub struct BufferPool {
    ctx: Arc<GpuContext>,
    free: Mutex<HashMap<(u64, wgpu::BufferUsages), Vec<Arc<wgpu::Buffer>>>>,
}

impl BufferPool {
    pub fn new(ctx: Arc<GpuContext>) -> Arc<Self>;

    /// Acquire a buffer >= size with at least the given usages.
    /// Pool returns a wrapper that, on drop, returns the buffer to the pool.
    pub fn acquire(&self, size: u64, usage: wgpu::BufferUsages) -> PooledBuffer;
}

pub struct PooledBuffer {
    pool: Arc<BufferPool>,
    buf: Option<Arc<wgpu::Buffer>>,
    key: (u64, wgpu::BufferUsages),
}
impl Deref for PooledBuffer { type Target = wgpu::Buffer; ... }
impl Drop for PooledBuffer { fn drop(&mut self) { /* return to pool */ } }
```

Size-class strategy: round size up to next power of two (or next multiple of 256 KiB above 1 MiB) to maximize reuse. Track per-frame stats in `tracing::debug!` for tuning.

### 2.3 `gpu::kernel` types

```rust
/// Element type of a binding. Engine maps these to WGSL types.
pub enum BindElem {
    PixelRgba8U32,        // u32 packed RGBA8
    PixelRgba16F,         // vec4<f16>  (later)
    PixelRgba32F,         // vec4<f32>
    Raw(u32),             // arbitrary stride in bytes (escape hatch)
}

pub enum BindAccess { Read, Write, ReadWrite }

pub struct ResourceDecl {
    pub name: &'static str,         // referenced in WGSL body
    pub elem: BindElem,
    pub access: BindAccess,
}

pub struct ParamDecl {
    pub name: &'static str,
    pub ty: ParamType,              // U32, I32, F32, Vec4F, Mat4 ...
}

pub enum DispatchShape {
    /// One thread per output pixel. workgroup_size is fixed at 8,8,1.
    PerPixel,
    /// Custom dimensions in pixels; engine computes div_ceil(dims, wg).
    Pixels { width_expr: &'static str, height_expr: &'static str },
}

pub struct KernelSig {
    pub name: &'static str,                // WGSL function name
    pub inputs: Vec<ResourceDecl>,
    pub outputs: Vec<ResourceDecl>,
    pub params: Vec<ParamDecl>,
    pub workgroup: (u32, u32, u32),
    pub dispatch: DispatchShape,
    pub class: KernelClass,                // PerPixel | Stencil { radius } | Reduce
    pub body_wgsl: &'static str,           // function body, using declared names
}

pub enum KernelClass {
    PerPixel,                              // 1 input pixel → 1 output pixel
    Stencil { radius: u32 },               // reads neighborhood
    Custom,                                // opt out of fusion
}
```

Kernels expose the sig plus an opaque "param pack" function that writes uniform bytes:

```rust
pub trait GpuKernel: Send + Sync {
    fn sig(&self) -> &'static KernelSig;
    fn write_params(&self, dst: &mut [u8]);
}
```

For blur:

```rust
pub struct BlurKernel { pub radius: u32 }
impl GpuKernel for BlurKernel {
    fn sig(&self) -> &'static KernelSig { &BLUR_SIG }
    fn write_params(&self, dst: &mut [u8]) {
        let p = BlurParams { radius: self.radius, _pad: 0 };
        dst[..16].copy_from_slice(bytemuck::bytes_of(&p));
    }
}
```

`BLUR_SIG` is a `static` with `body_wgsl` referencing the declared input/output/param names. Engine generates the WGSL prologue (struct decls, group/binding annotations, helpers).

### 2.4 `gpu::codegen`

Input: `&[&KernelSig]` (for fusion: ordered list to fuse). Output: a `String` of full WGSL.

Single-kernel emission produces:

```wgsl
struct Params0 {
    radius: u32,
    _pad: u32,
};
@group(0) @binding(0) var<uniform> params0: Params0;
@group(0) @binding(1) var<storage, read>       in0_src:  array<u32>;
@group(0) @binding(2) var<storage, read_write> out0_dst: array<u32>;
// + helpers (unpack, pack) when any binding is PixelRgba8U32

// inlined body referencing params0.radius, in0_src[...], out0_dst[...]
@compute @workgroup_size(8, 8, 1)
fn entry(@builtin(global_invocation_id) gid: vec3<u32>) {
    /* body_wgsl pasted, with name substitution if needed */
}
```

Naming convention enforced: the body uses the raw declared names (`src`, `dst`, `radius`) and codegen rewrites or aliases. Simplest path: declare everything in the global namespace using exactly the user's names, no rewriting; codegen errors if two kernels collide on names (Phase 3 problem).

### 2.5 `gpu::scheduler`

```rust
pub struct Scheduler {
    ctx: Arc<GpuContext>,
    pool: Arc<BufferPool>,
    pipeline_cache: Mutex<HashMap<u64, Arc<wgpu::ComputePipeline>>>,  // key = sig hash
    bgl_cache: Mutex<HashMap<u64, Arc<wgpu::BindGroupLayout>>>,
    encoder: Mutex<Option<wgpu::CommandEncoder>>,
    keepalive: Mutex<Vec<Arc<wgpu::Buffer>>>,
}

impl Scheduler {
    pub fn new(ctx: Arc<GpuContext>, pool: Arc<BufferPool>) -> Arc<Self>;

    /// Run a single kernel against a tile or neighborhood. Returns the output buffer.
    pub fn dispatch_one(
        &self,
        kernel: &dyn GpuKernel,
        inputs: &[&GpuBuffer],
        out_size: u64,
        dispatch_dims: (u32, u32, u32),
    ) -> Result<Arc<wgpu::Buffer>, Error>;

    /// Flush encoder, submit, drop keepalive.
    pub fn flush(&self);
}

pub fn global() -> &'static Arc<Scheduler>;  // singleton initialized after GpuContext
```

Internal flow of `dispatch_one`:
1. Hash `kernel.sig()` → look up or create `(BindGroupLayout, ComputePipeline)`.
2. `pool.acquire` for output buffer with usages `STORAGE | COPY_SRC | COPY_DST`.
3. `pool.acquire` for params uniform buffer; `kernel.write_params(slice)`; `queue.write_buffer`.
4. Build `BindGroup` (no caching for now — bind group identity ties to buffer identity which churns; cache later if hot).
5. `encoder.begin_compute_pass`, set pipeline + bind group, `dispatch_workgroups(...)`, end pass.
6. Push pooled buffers to `keepalive` until `flush()` runs.
7. Auto-flush every N tiles (configurable, default 16) — port the `BATCH_SIZE` logic from current `BlurKernelGpuRunner`.

### 2.6 Stage shape post-refactor

`pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs` shrinks to ~50 lines:

```rust
pub struct BlurKernelGpu { pub radius: u32 }

impl Stage for BlurKernelGpu { /* trivial */ }

pub struct BlurKernelGpuRunner { kernel: BlurKernel }

impl OperationRunner for BlurKernelGpuRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("BlurKernelGpu expected Neighborhood")),
        };

        let sched = gpu::scheduler::global();
        let src = build_neighborhood_src_buffer(sched, &nbhd)?;  // helper fills scratch
        let out = sched.dispatch_one(
            &self.kernel,
            &[&src],
            (nbhd.center.width * nbhd.center.height * 4) as u64,
            (nbhd.center.width.div_ceil(8), nbhd.center.height.div_ceil(8), 1),
        )?;

        let gbuf = GpuBuffer::new(out, /* size */);
        emit.emit(Item::Tile(Tile::new(nbhd.center, nbhd.meta, Buffer::Gpu(gbuf))));
        Ok(())
    }
}
```

The scratch-source-buffer assembly stays in the stage for now (since it is neighborhood-specific) but uses `pool.acquire` instead of raw `device.create_buffer`. Long-term, neighborhood reassembly itself becomes a kernel.

### 2.7 Migration of `Upload`

`pixors-engine/src/pipeline/exec/upload.rs` line 53 `create_buffer_init` becomes `pool.acquire` + `queue.write_buffer`. No fundamental change, just goes through the pool.

### 2.8 P2 acceptance checklist

- [ ] `BlurKernelGpu` runs via the scheduler; output bit-identical to pre-refactor (snapshot test on `pixors_blur_roundtrip.png`).
- [ ] No stage file imports `wgpu::util::DeviceExt` or calls `device.create_*` directly. Search: `rg "device\.(create_buffer|create_bind_group|create_compute_pipeline)" pixors-engine/src/pipeline/exec/`.
- [ ] Buffer pool reuse rate logged, > 70% on second blur invocation.
- [ ] `cargo clippy --workspace` clean.

---

## Phase 3 — Kernel Fusion

### 3.1 Fusion rules

Two adjacent kernels A → B fuse iff:
- Both are `KernelClass::PerPixel`, OR
- A is `PerPixel` and B is `Stencil` with the stencil reading B's input that is A's output, OR
- A is `Stencil` and B is `PerPixel` reading A's output.
- Output of A is consumed only by B (no other readers — exec graph fan-out check).
- Dispatch shape compatible (same per-pixel grid).

`PerPixel + PerPixel` is the easy case for v1. Stencil fusion is v2.

### 3.2 Fusion graph pass

Add a new exec_graph pre-pass: `pixors-engine/src/pipeline/exec_graph/fusion.rs`. Walks the graph, replaces fusable chains with a synthetic `FusedKernelStage` that holds an ordered `Vec<Box<dyn GpuKernel>>` and a single `KernelSig` produced by `gpu::codegen::fuse(&[&KernelSig])`.

`codegen::fuse` rules:
- Concatenate input/output decls, **dropping** any output of A that is consumed only as input of B (the fused intermediate becomes a local `var`).
- Concatenate params with index-suffixed names (`params0`, `params1`).
- Emit a single `@compute` entry that:
  ```wgsl
  let pix0 = unpack(in0_src[idx]);
  let pix1 = body_a(pix0, params0);    // user's body wrapped as a function
  let pix2 = body_b(pix1, params1);
  out_final[idx] = pack(pix2);
  ```

Constraint: kernel bodies must declare a fusable form. Add to `GpuKernel` trait:

```rust
/// For PerPixel kernels: WGSL fragment that maps `vec4<f32>` → `vec4<f32>`
/// referencing the kernel's params struct. None ⇒ not fusable.
fn fusable_body(&self) -> Option<&'static str>;
```

Kernels that opt in expose a function-form body. The standalone path stays as `body_wgsl` for non-fused dispatches.

### 3.3 P3 acceptance checklist

- [ ] A pipeline with `ColorConvert → ColorConvert` (two color converts) compiles to ONE shader, ONE dispatch — verify with `tracing` log of dispatch count.
- [ ] Fused output bit-identical to two-pass output.
- [ ] Graph viz exporter (if exists) shows the synthetic node.

---

## Phase 4 — Slang adoption

### 4.1 Toolchain

Add to `pixors-engine/Cargo.toml`:
```toml
[build-dependencies]
shader-slang = "0.x"   # crate name; verify on crates.io
```

Or vendor `slangc` binary. The Slang project (https://github.com/shader-slang/slang) ships official Rust bindings.

### 4.2 File layout

```
pixors-engine/shaders/
├── core.slang              # helpers: pack/unpack rgba8, color spaces
├── blur.slang
├── color_convert.slang
└── kernel_iface.slang      # interface IKernel { float4 run(KernelCtx ctx); }
```

`build.rs` invokes `slangc` per `.slang` file → emits `.wgsl` into `OUT_DIR/shaders/`. Each kernel sig replaces `body_wgsl: &'static str` with `body_wgsl: include_str!(concat!(env!("OUT_DIR"), "/shaders/blur.wgsl"))`.

### 4.3 Slang kernel interface

```slang
struct KernelCtx {
    uint2 gid;
    uint2 dim;
};

interface IKernel {
    float4 run(KernelCtx ctx, float4 pixel);
}

// blur.slang
struct BlurParams { uint radius; uint _pad; };
ParameterBlock<BlurParams> params;
StructuredBuffer<uint> src;
RWStructuredBuffer<uint> dst;

struct BlurKernel : IKernel {
    float4 run(KernelCtx ctx, float4 pixel) {
        // … box filter via src buffer …
    }
}
```

Fusion in Slang version (Phase 4.5): a generic shader

```slang
[shader("compute")]
[numthreads(8,8,1)]
void cs_chain<each K : IKernel>(uint3 gid : SV_DispatchThreadID) {
    KernelCtx ctx = { gid.xy, ... };
    float4 v = load(gid.xy);
    expand each K { v = K::run(ctx, v); }
    store(gid.xy, v);
}
```

Specialize at runtime via Slang's compose API. This supersedes string-based fusion from Phase 3 — Phase 3 stays as the fallback for kernels that haven't been ported to Slang yet.

### 4.4 P4 acceptance checklist

- [ ] `cargo build` invokes `slangc`, generates `.wgsl` artifacts.
- [ ] Existing tests still pass (Slang-generated WGSL is functionally equivalent).
- [ ] At least 2 kernels migrated (blur + color_convert).

---

## 4. Cross-cutting items

### 4.1 Testing

- Add `pixors-engine/src/gpu/tests.rs` cases for `BufferPool` (acquire/release roundtrip, size-class merging).
- Add a snapshot test: run blur via old direct path (kept temporarily on a feature flag) vs new scheduler path, compare bytes.
- Add `pixors-desktop` doesn't have tests today — leave that gap.

### 4.2 Logging

Every new module emits `tracing::debug!` with consistent prefix:
- `[pixors] gpu::pool: ...`
- `[pixors] gpu::scheduler: ...`
- `[pixors] viewport::tiled_texture: ...`

Match the existing `[pixors] blur_kernel_gpu: ...` style (see `blur_kernel/gpu.rs` line 80–86).

### 4.3 Clippy lints to watch

Repo workspace already denies `needless_borrow`, `unnecessary_cast`, `manual_div_ceil`, `slow_vector_initialization` (see `CLAUDE.md`). The `BufferPool` and codegen will produce a lot of arithmetic — use `usize::div_ceil` not `(x + n - 1) / n`. Use `Vec::with_capacity` not `Vec::new()` + repeated `push` when size known.

### 4.4 `CONTRIBUTING.md` update

After Phase 2 ships, document the new GPU stage authoring flow in `CONTRIBUTING.md`. Replace any mention of stages owning wgpu resources with the new declarative pattern.

### 4.5 Commits

- One logical change per commit. P1 produces ~6 commits (one per new file, one for delete, one for app integration).
- Conventional commits as per `CLAUDE.md`. Examples:
  - `feat(viewport): add TiledTexture with mip chain`
  - `feat(viewport): mip generation via compute pass`
  - `refactor(viewport): split engine.rs into viewport submodules`
  - `feat(engine): add tile_sink stage replacing display_sink`
  - `refactor(gpu): introduce BufferPool, route Upload through it`
  - `feat(gpu): KernelSig + Scheduler, port BlurKernelGpu`

### 4.6 Risks & open questions

1. **wgpu version mismatch** between iced and pixors-engine. Verify `pixors-desktop/Cargo.toml` and `pixors-engine/Cargo.toml` resolve to the same wgpu version. If not, P2.5 cross-device handoff is impossible without copying through CPU. Check `cargo tree -p wgpu` before starting.
2. **Anisotropy support**: if iced's wgpu device wasn't requested with anisotropy enabled, `anisotropy_clamp > 1` will panic at sampler creation. iced exposes no hook to customize device features. Mitigation: stick to trilinear (anisotropy=1).
3. **Storage texture support**: `rgba8unorm` storage write requires `TEXTURE_FORMAT_RGBA8_UNORM_STORAGE` adapter feature. Fallback to render-pipeline mip generation if absent.
4. **Slang Rust bindings** maturity: verify the `shader-slang` crate state before committing P4. If too immature, vendor `slangc` and parse its output instead.

---

## 5. Order of operations for the executing agent

1. Read `CLAUDE.md`, `CONTRIBUTING.md`, this plan in full.
2. Verify wgpu version alignment (§4.6 #1) — if mismatched, STOP and report.
3. Branch `feature/viewport-tiles` off current `feature/phase9`.
4. Implement P1 files in order: `tiled_texture.rs` → `mip_builder.rs` → `pipeline.rs` → `program.rs` → `tile_sink.rs` → `mod.rs` → app wiring → delete `engine.rs` and `display_sink.rs`.
5. Run `cargo check`, `cargo test`, `cargo clippy --workspace`, `cargo run -p pixors-desktop` after each file.
6. Open PR for P1. Stop. Wait for review.
7. After P1 merges, branch P2. Repeat.

Do not skip ahead. Do not bundle phases. Do not commit broken intermediate states; if a phase's mid-state doesn't compile, keep it in your worktree and only commit working slices.

---

## 6. Appendix — file-by-file change matrix

| File | Phase | Action |
|---|---|---|
| `pixors-desktop/src/main.rs` | P1 | Remove `mod engine;`, add `mod viewport::pipeline;` etc. |
| `pixors-desktop/src/engine.rs` | P1 | DELETE (split into `viewport/*`) |
| `pixors-desktop/src/viewport/mod.rs` | P1 | Expand to re-export new submodules |
| `pixors-desktop/src/viewport/camera.rs` | P1 | Keep as-is |
| `pixors-desktop/src/viewport/tiled_texture.rs` | P1 | NEW |
| `pixors-desktop/src/viewport/mip_builder.rs` | P1 | NEW |
| `pixors-desktop/src/viewport/pipeline.rs` | P1 | NEW (was `EnginePipeline`) |
| `pixors-desktop/src/viewport/program.rs` | P1 | NEW (was `EngineProgram`) |
| `pixors-desktop/src/ui/app.rs` (line 194) | P1 | Replace `display_sink::init_buffer` block |
| `pixors-engine/src/pipeline/exec/display_sink.rs` | P1 | DELETE after callers migrate |
| `pixors-engine/src/pipeline/exec/tile_sink.rs` | P1 | NEW |
| `pixors-engine/src/pipeline/exec/mod.rs` (line 5, 18, 68) | P1 | Replace `display_sink` ⇒ `tile_sink` in `mod`/`pub use`/`ExecNode` |
| `pixors-engine/src/gpu/context.rs` (line 67) | P1 | Bump features/limits if needed |
| `pixors-engine/src/gpu/pool.rs` | P2 | NEW |
| `pixors-engine/src/gpu/kernel.rs` | P2 | NEW |
| `pixors-engine/src/gpu/codegen.rs` | P2 | NEW |
| `pixors-engine/src/gpu/scheduler.rs` | P2 | NEW |
| `pixors-engine/src/gpu/kernels/blur.rs` | P2 | DELETE (replaced by sig + body) |
| `pixors-engine/src/gpu/kernels/blur.wgsl` | P2 | NEW (extracted body) |
| `pixors-engine/src/gpu/mod.rs` | P2 | Add new pub mods |
| `pixors-engine/src/pipeline/exec/blur_kernel/gpu.rs` | P2 | Shrink to ~50 lines via scheduler |
| `pixors-engine/src/pipeline/exec/upload.rs` | P2 | Route through pool |
| `pixors-engine/src/pipeline/exec_graph/fusion.rs` | P3 | NEW |
| `pixors-engine/shaders/*.slang` | P4 | NEW |
| `pixors-engine/build.rs` | P4 | NEW (slangc invocation) |
| `pixors-engine/Cargo.toml` | P4 | Add `shader-slang` build-dep |

End of plan.
