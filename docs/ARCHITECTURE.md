# Pixors Architecture

> Authoritative reference. Updated: Phase 9 — executor-based, engine deprecated.

---

## 1. The Big Picture

Pixors is a desktop image editor with a **Rust executor** (`pixors-executor`) for all
heavy lifting and an **Iced GUI** (`pixors-desktop`) for rendering.

### Data flow from file open to pixel on screen

```
File on disk
  │
  ▼
ImageFile::open(path)          ← reads metadata only (dimensions, layers, color space)
  │
  ▼
Desktop builds ExecGraph       ← Source → Blur → Blur → Sink
  │
  ▼
Pipeline::compile(&graph)      ← inserts Upload/Download bridges, detects GPU chains
  │
  ▼
pipeline.run()                 ← spawns threads, tiles flow through channels
  │
  ├── CPU chain: image decode → to_scanline → color convert → blur → ...
  │
  ├── GPU chain: upload → compute dispatch (SPIR-V) → download
  │
  └── Sink: ViewportSink writes tiles directly to GPU texture, displayed by Iced
```

Every step is **tile-granularity**. Channels use bounded `sync_channel` for
backpressure. No full-image buffer exists anywhere after decode.

---

## 2. Crate Map

```
pixors/
├── pixors-executor/    # Backend: data types, graph, stages, GPU, runtime
└── pixors-desktop/     # Iced GUI: viewport, UI components, file dialog
```

---

## 3. Module Map — pixors-executor

```
src/
├── data/              # Units of work flowing through the pipeline
│   ├── buffer.rs          Buffer { Cpu(Arc<Vec<u8>>) | Gpu(GpuBuffer) }
│   ├── tile.rs            Tile, TileCoord
│   ├── scanline.rs        ScanLine
│   ├── neighborhood.rs    Neighborhood, EdgeCondition
│   └── device.rs          Device { Cpu, Gpu }
│
├── graph/             # Execution graph (DAG of stages)
│   ├── graph.rs           ExecGraph, StageId, EdgePorts
│   ├── emitter.rs         Emitter<T> — push collector between stages
│   ├── item.rs            Item enum { Tile | ScanLine | Neighborhood }
│   └── executor.rs        Simple topological executor
│
├── stage.rs           # Stage trait + StageNode wrapper enum
│   ├── Stage trait        kind(), ports(), hints(), cpu_kernel(), gpu_kernel_descriptor()
│   ├── StageNode          Source(SourceNode) | Sink(SinkNode) | Operation(OperationNode)
│   ├── CpuKernel trait    process(item, emitter) + finish(emitter)
│   ├── GpuKernelDescriptor  SPIR-V + entry point + binding layout + param_size
│   ├── BufferAccess       ReadOnly | ReadWriteInPlace | ReadTransform
│   └── StageRole          Source | Operation | Sink
│
├── source/            # Source stages (produce data from nothing)
│   ├── image_file_source.rs  ImageFileSource — decodes PNG/TIFF to scanlines
│   ├── file_decoder.rs       FileDecoder (legacy)
│   └── cache_reader.rs       CacheReader (stub)
│
├── sink/              # Sink stages (consume data, produce side effects)
│   ├── viewport.rs          ViewportSink — GPU-direct tile writes to texture
│   ├── tile_sink.rs         TileSink — callback-based tile delivery
│   ├── png_encoder.rs       PngEncoder — PNG export
│   └── cache_writer.rs      CacheWriter (stub)
│
├── operation/         # Operation stages (transform data)
│   ├── blur.rs              Blur — separable box blur, CPU + GPU (SPIR-V)
│   ├── color/cpu.rs         ColorConvert (stub)
│   ├── composition/         Layer composition (stubs)
│   ├── transfer/
│   │   ├── upload.rs        Upload — CPU → GPU data transfer
│   │   └── download.rs      Download — GPU → CPU data transfer
│   └── data/
│       ├── to_scanline.rs   TileToScanline
│       ├── to_tile.rs       ScanLineAccumulator
│       └── to_neighborhood.rs  NeighborhoodAgg
│
├── gpu/               # GPU infrastructure
│   ├── context.rs          GpuContext { device, queue }, try_init(), gpu_available()
│   ├── kernel.rs           KernelSignature, GpuKernel trait, BindingElement, etc.
│   ├── pool.rs             BufferPool — size-classed buffer recycling
│   └── scheduler.rs        Scheduler — pipeline cache, dispatch_one(), flush()
│
├── runtime/           # Pipeline execution engine
│   ├── pipeline.rs         Pipeline::compile() + run() — chain detection, threading
│   ├── cpu.rs              CpuChainRunner — sequential kernels in one thread
│   ├── gpu.rs              GpuChainRunner — GPU dispatch per item
│   ├── runner.rs           Runner trait, ItemSender/ItemReceiver, CHANNEL_BOUND
│   └── event.rs            PipelineEvent { Progress, Done, Error }
│
├── model/             # Shared abstractions (no pipeline dependency)
│   ├── color/              ColorSpace, TransferFn, RgbPrimaries, chromaticity, matrices
│   ├── image/
│   │   ├── image_file.rs   ImageFile::open() — metadata, LayerFileInfo, source() plug
│   │   ├── document/       Image, Layer, BlendMode, Orientation
│   │   ├── buffer.rs       BufferDesc, PlaneDesc, ImageBuffer, SampleFormat
│   │   ├── tile.rs         TileCoord, TileGrid
│   │   └── mip.rs          MipLevel, MipPyramid
│   ├── io/
│   │   ├── png.rs           PNG decode
│   │   └── tiff.rs          TIFF decode (read only)
│   ├── pixel/               PixelFormat, PixelMeta, Rgba, Rgb, Gray, AlphaPolicy
│   └── storage/             Disk-backed tile persistence
│
├── error.rs           # Error enum (thiserror)
└── utils.rs           # ApproximateEq, debug_stopwatch! macro
```

---

## 4. Core Abstractions

### 4.1 Stage trait

Every node in the execution graph implements `Stage`:

```rust
pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpec;
    fn hints(&self) -> StageHints;
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>>;
    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor>;
}
```

- `cpu_kernel()` — returns a CPU runner. Used by `CpuChainRunner` in the pipeline.
- `gpu_kernel_descriptor()` — returns SPIR-V + binding layout. Used by `GpuChainRunner`.

A stage that returns BOTH can run on either device — the pipeline picks based on
`hints().prefers_gpu` and GPU availability.

### 4.2 StageNode enum

Three categories wrapped in a single enum for graph storage:

```rust
pub enum StageNode {
    Source(SourceNode),       // FileDecoder, ImageFileSource, CacheReader
    Sink(SinkNode),           // ViewportSink, TileSink, PngEncoder, CacheWriter
    Operation(OperationNode), // Blur, ColorConvert, Upload, Download, ...
}
```

### 4.3 CpuKernel trait

```rust
pub trait CpuKernel: Send {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error>;
    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> { Ok(()) }
}
```

Kernels receive one `Item` at a time, process it, and emit zero or more output items.
`finish()` is called once when the input stream ends — used to flush accumulated data.

### 4.4 GpuKernelDescriptor

```rust
pub struct GpuKernelDescriptor {
    pub spirv: &'static [u8],
    pub entry_point: &'static str,
    pub binding_layout: BindingLayout,
    pub param_size: u64,
    pub write_params: Box<dyn Fn(&mut [u8]) + Send + Sync>,
}
```

The runtime pipeline reads this descriptor, creates compute pipelines, binds resources,
and dispatches. Each GPU kernel in a chain processes the output of the previous one.

### 4.5 BufferAccess

Controls how the pipelin manages memory around a stage:

```rust
pub enum BufferAccess {
    ReadOnly,           // Input is not modified (sinks, sources)
    ReadWriteInPlace,   // Modifies input in-place; copy first if shared
    ReadTransform,      // Reads input, writes to new buffer (most operations)
}
```

---

## 5. Pipeline Execution

### 5.1 Pipeline::compile()

1. **Clone the ExecGraph** into a mutable `StableDiGraph<StageNode, EdgePorts>`
2. **Assign devices**: each node gets Cpu or Gpu based on `hints().prefers_gpu` and GPU availability
3. **Insert Upload/Download bridges** where adjacent nodes are on different devices
4. **Detect chains**: consecutive same-device nodes with no branching are merged into "chains"
5. **Build channels**: inter-chain edges become `sync_channel<Option<Item>>` (bounded, backpressure)
6. **Create runners**: each chain becomes a `CpuChainRunner` or `GpuChainRunner`

### 5.2 Pipeline::run()

Spawns one OS thread per chain via `std::thread::scope`. Each runner:
1. If source (no inputs): kicks off with a dummy item, runs kernels, emits results
2. If regular: loops on input channel, processes each item through all chain kernels
3. Sends `None` (end-of-stream sentinel) to all output channels when done

### 5.3 CpuChainRunner

Holds an ordered list of `CpuKernel` stages. For each item:
1. Creates `Emitter`
2. Calls `kernel[0].process(item, emit)`
3. Takes `emit.into_items()` → calls `kernel[1].process(...)` with each result
4. Fan-out: if multiple output channels, clones each output item across all channels

### 5.4 GpuChainRunner

For each item, extracts GPU buffers from the input, dispatches each kernel descriptor
through the GPU scheduler, and copies the final output back.

---

## 6. Example: Blur

```rust
// pixors-executor/src/operation/blur.rs

pub struct Blur { pub radius: u32 }

impl Stage for Blur {
    fn kind(&self) -> &'static str { "blur" }
    fn ports(&self) -> &'static PortSpec { &BLUR_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: true }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(BlurCpuRunner { radius: self.radius }))
    }
    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        let radius = self.radius;
        Some(GpuKernelDescriptor {
            spirv: BLUR_SPIRV,
            entry_point: "cs_blur",
            binding_layout: BindingLayout { groups: vec![
                BindingGroup { bindings: vec![
                    Binding { index: 0, buffer: BufferBinding::StorageReadOnly },
                    Binding { index: 1, buffer: BufferBinding::StorageReadWrite },
                ]},
                BindingGroup { bindings: vec![
                    Binding { index: 0, buffer: BufferBinding::Uniform },
                ]},
            ]},
            param_size: 16,
            write_params: Box::new(move |dst| {
                let p = BlurParams { width: 0, height: 0, radius, _pad: 0 };
                dst.copy_from_slice(bytemuck::bytes_of(&p));
            }),
        })
    }
}
```

Desktop usage:

```rust
let blur = graph.add_stage(StageNode::Operation(
    OperationNode::Blur(Blur { radius: 8 })
));
```

---

## 7. GPU Integration

The executor manages its own wgpu instance (`gpu::context::GpuContext`) separate from
Iced's wgpu. Both use the same physical GPU (NVIDIA GTX 1650 in dev). The `ViewportSink`
bridges them: it receives GPU tiles from the executor's wgpu and copies them to a shared
texture readable by Iced's render pipeline.

Pipeline layout for compute shaders has two bind groups:
- **Group 0**: storage buffers (read-only inputs + read-write outputs)
- **Group 1**: uniform params

This matches the slang SPIR-V shader convention:
```slang
@group(0) @binding(0) var<storage, read>  src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(1) @binding(0) var<uniform> params: BlurParams;
```

---

## 8. Desktop Integration

### 8.1 Opening an image

```rust
// pixors-desktop/src/ui/file_ops.rs

pub fn open_and_run(pending: &Arc<PendingTileWrites>) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new().pick_file()?;
    let image = ImageFile::open(&path)?;
    let (w, h) = (image.width, image.height);

    let mut graph = ExecGraph::new();
    let src = graph.add_stage(StageNode::Source(
        SourceNode::ImageFile(image.source(0))
    ));
    let blur = graph.add_stage(StageNode::Operation(
        OperationNode::Blur(Blur { radius: 8 })
    ));
    let sink = graph.add_stage(StageNode::Sink(
        SinkNode::Viewport(ViewportSink { width: w, height: h })
    ));
    graph.add_edge(src, blur, EdgePorts::default());
    graph.add_edge(blur, sink, EdgePorts::default());

    let pipeline = Pipeline::compile(&graph)?;
    std::thread::spawn(move || { let _ = pipeline.run(None); });
    Ok((w, h, path))
}
```

### 8.2 Display

The `ViewportSink` writes tiles directly to a GPU texture. Iced's `ViewportPipeline`
binds that texture each frame via `prepare()` → rebind → `render()` with a fullscreen
triangle shader that samples the texture with camera pan/zoom.

---

## 9. Color Science

### 9.1 ColorSpace

```rust
pub struct ColorSpace {
    primaries: RgbPrimaries,
    white_point: WhitePoint,
    transfer: TransferFn,
}
```

Predefined constants: `SRGB`, `LINEAR_SRGB`, `REC709`, `REC2020`, `DISPLAY_P3`,
`ACES2065_1`, `ACES_CG`, etc.

`matrix_to(dst)` computes the linear-RGB→linear-RGB transformation matrix.
`transfer` provides to_linear/from_linear conversion (gamma, PQ, HLG, sRGB curve).

### 9.2 Pixel types

`PixelFormat` enum + `PixelMeta` (format + color space + alpha policy).
Alpha policies: `Straight`, `PremultiplyOnPack`, `OpaqueDrop`.

---

## 10. Adding a New Operation

1. **Define the stage struct** with parameters
2. **Implement `Stage`** — `ports()`, `hints()`, `cpu_kernel()`, optionally `gpu_kernel_descriptor()`
3. **Implement `CpuKernel`** — `process(item, emit)` with the transform logic
4. **Add to the enum** — `OperationNode`, `SourceNode`, or `SinkNode`
5. **Add match arms** to the enum's `Stage` impl
6. **Desktop usage** — `StageNode::Operation(OperationNode::MyOp(MyOp { ... }))`

---

## 11. Roadmap

| Feature | Status | Priority |
|---------|--------|----------|
| ImageFile + ImageFileSource | Done | — |
| Blur (CPU + GPU) | Done | — |
| GPU scheduler (multi-pass) | Done | — |
| Pipeline (multi-threaded, chain detection) | Done | — |
| ViewportSink (GPU-direct writes) | Done | — |
| Desktop → executor integration | Done | — |
| ColorConvert (real implementation) | Planned | High |
| Layer composition/blend | Planned | High |
| Cache (reader + writer) | Planned | Medium |
| TIFF write | Planned | Medium |
| PNG export pipeline | Planned | Medium |
| GPU ColorConvert (SPIR-V shader) | Future | Low |
| Non-destructive editing (operation stack) | Future | Low |
| Selections, masks | Future | Low |
