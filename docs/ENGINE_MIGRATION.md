# Engine Migration — Remaining Features

This document maps every feature from the deprecated `pixors-engine` to its
current or planned location in `pixors-executor`, with concrete implementation
examples.

---

## 1. Layer Composition / Blend

**Engine**: `composite/` (layer blend modes, opacity, merge)
**Executor**: `operation/composition/` (stubs — empty)

### Design

Composition takes N layers + blend mode and merges them into one output. Each
layer has pixel data, opacity, and a blend mode. The blend pipelin is:

```
Layer0 ──┐
Layer1 ──┼── Blend ──► Output
Layer2 ──┘
```

Since our pipeline is streaming (tile-by-tile, single-input), the composition
stage receives tiles from one layer at a time and composites them onto an
accumulator buffer. The accumulator is initialized with the bottom layer, then
subsequent layers are blended on top.

### Code

```rust
// pixors-executor/src/operation/composition/cpu.rs

use std::sync::Arc;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};

use crate::data::Buffer;
use crate::data::Tile;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::image::document::BlendMode;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};

static COMP_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static COMP_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static COMP_PORTS: PortSpec = PortSpec { inputs: COMP_INPUTS, outputs: COMP_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Composition {
    pub width: u32,
    pub height: u32,
    pub target_layer: usize,        // 0 = base, N = blend on top
    pub blend_mode: BlendMode,
    pub opacity: f32,
}

impl Stage for Composition {
    fn kind(&self) -> &'static str { "composition" }
    fn ports(&self) -> &'static PortSpec { &COMP_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: false }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(CompositionRunner::new(self.width, self.height, self.blend_mode, self.opacity)))
    }
}

pub struct CompositionRunner {
    width: u32,
    height: u32,
    blend_mode: BlendMode,
    opacity: f32,
    accumulator: Mutex<Option<Arc<Vec<u8>>>>,
    current_layer: Mutex<usize>,
}

impl CpuKernel for CompositionRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let tile = match item { Item::Tile(t) => t, _ => return Ok(()), };
        let data = tile.data.as_cpu_slice().unwrap();

        let mut acc = self.accumulator.lock().unwrap();
        if acc.is_none() {
            // First layer: copy directly
            let full = vec![0u8; self.width as usize * self.height as usize * 4];
            *acc = Some(Arc::new(full));
        }

        let acc_data = Arc::get_mut(acc.as_mut().unwrap()).unwrap();
        let stride = self.width as usize * 4;
        let px = tile.coord.px as usize;
        let py = tile.coord.py as usize;
        let tw = tile.coord.width as usize;
        let th = tile.coord.height as usize;

        for row in 0..th {
            let dst_off = (py + row) * stride + px * 4;
            let src_off = row * tw * 4;
            let len = tw * 4;

            match self.blend_mode {
                BlendMode::Normal => {
                    // src OVER dst with alpha
                    for i in (0..len).step_by(4) {
                        let sa = data[src_off + i + 3] as f32 / 255.0 * self.opacity;
                        for c in 0..4 {
                            let dst_val = acc_data[dst_off + i + c] as f32;
                            let src_val = data[src_off + i + c] as f32;
                            acc_data[dst_off + i + c] =
                                (src_val * sa + dst_val * (1.0 - sa)) as u8;
                        }
                    }
                }
            }
        }

        // Emit the blended result tile
        let tile_data = Vec::from(&acc_data[/* tile region */]);
        emit.emit(Item::Tile(Tile::new(tile.coord, tile.meta, Buffer::cpu(tile_data))));
        Ok(())
    }
}
```

### Integration

Add to `OperationNode`:

```rust
pub enum OperationNode {
    // ...
    Composition(composition::cpu::Composition),
}
```

Desktop usage (simplified):

```rust
// Composite layer 1 over layer 0
let src0 = graph.add_stage(StageNode::Source(layer0.source()));
let src1 = graph.add_stage(StageNode::Source(layer1.source()));
let comp = graph.add_stage(StageNode::Operation(
    OperationNode::Composition(Composition {
        width: w, height: h,
        target_layer: 1,
        blend_mode: BlendMode::Normal,
        opacity: 1.0,
    })
));
graph.add_edge(src0, comp, EdgePorts::default());
graph.add_edge(src1, comp, EdgePorts::default());
```

**Caveat**: This design expects tiles from ALL layers to arrive at the
composition stage. The current Pipeline supports DAGs with fan-in
(multiple edges → one stage), but the CompositionRunner must accumulate
tiles from all layers before emitting. A simpler two-layer merge
(blend layer N onto layer N-1) is more practical for the existing
single-chain model.

---

## 2. Cache (Disk Cache)

**Engine**: `state_graph/cache.rs`, `cache_reader.rs`, `cache_writer.rs`
**Executor**: `source/cache_reader.rs`, `sink/cache_writer.rs` (stubs)

### Design

Cache stores intermediate pipeline results to disk, avoiding recomputation.
Use case: after blurring a 100MP image, cache the result so reopening or
adjusting other parameters doesn't re-run the blur.

```
[Pipeline stages...] → CacheWriter → [disk]
                       CacheReader → [pipeline resumes...]
```

### CacheWriter

```rust
// pixors-executor/src/sink/cache_writer.rs

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use crate::data::Tile;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWriter {
    pub cache_dir: PathBuf,
    pub key: String,
}

impl Stage for CacheWriter {
    fn kind(&self) -> &'static str { "cache_writer" }
    // ...
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(CacheWriterRunner {
            dir: self.cache_dir.clone(),
            key: self.key.clone(),
        }))
    }
}

pub struct CacheWriterRunner {
    dir: PathBuf,
    key: String,
}

impl CpuKernel for CacheWriterRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        let tile = match item { Item::Tile(t) => t, _ => return Ok(()), };
        let data = tile.data.as_cpu_slice().ok_or_else(|| Error::internal("GPU tile"))?;

        let tile_path = self.dir.join(format!(
            "{}/tile_{}_{}_{}_{}.raw",
            self.key, tile.coord.px, tile.coord.py, tile.coord.width, tile.coord.height
        ));
        fs::create_dir_all(tile_path.parent().unwrap())?;
        File::create(&tile_path)?.write_all(data)?;
        Ok(())
    }
}
```

### CacheReader

```rust
// pixors-executor/src/source/cache_reader.rs

use crate::data::{Buffer, ScanLine, Tile};
use crate::model::pixel::meta::PixelMeta;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_dir: PathBuf,
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
}

impl Stage for CacheReader {
    fn kind(&self) -> &'static str { "cache_reader" }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(CacheReaderRunner { /* ... */ }))
    }
}

impl CpuKernel for CacheReaderRunner {
    fn process(&mut self, _item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        // Iterate tile grid, read each from disk, emit
        for ty in 0..self.grid_h {
            for tx in 0..self.grid_w {
                let path = self.dir.join(format!(
                    "{}/tile_{}_{}_{}_{}.raw",
                    self.key, tx * self.tile_size, ty * self.tile_size,
                    self.tile_size, self.tile_size
                ));
                let data = fs::read(&path)?;
                let coord = TileCoord::new(tx * self.tile_size, ty * self.tile_size,
                                           self.tile_size, self.tile_size, tx, ty);
                emit.emit(Item::Tile(Tile::new(coord, self.meta, Buffer::cpu(data))));
            }
        }
        Ok(())
    }
}
```

---

## 3. TIFF Write

**Current**: `model/io/tiff.rs` reads only.
**Needed**: TIFF encoding for export.

```rust
// pixors-executor/src/model/io/tiff.rs — add to existing file

pub fn write_tiff_rgba8(path: &Path, data: &[u8], width: u32, height: u32)
    -> Result<(), Error>
{
    use tiff::encoder::{TiffEncoder, colortype};
    let file = File::create(path)?;
    let mut encoder = TiffEncoder::new(file)?;
    encoder.write_image::<colortype::RGBA8>(width, height, data)?;
    Ok(())
}
```

For streaming tile-by-tile, the PngEncoder sink pattern applies:

```rust
// pixors-executor/src/sink/tiff_encoder.rs

pub struct TiffEncoder {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

impl CpuKernel for TiffEncoderRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        // Accumulate tiles, write full image on finish()
    }
}
```

---

## 4. PNG Export Pipeline

Desktop usage to export a processed image:

```rust
// pixors-desktop/src/ui/export.rs (new file)

use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::{SinkNode, png_encoder::PngEncoder};

pub fn export_png(image: &ImageFile, output: &Path) -> Result<(), String> {
    let w = image.width;
    let h = image.height;

    let mut graph = ExecGraph::new();
    let src = graph.add_stage(StageNode::Source(
        SourceNode::ImageFile(image.source(0))
    ));
    let sink = graph.add_stage(StageNode::Sink(
        SinkNode::PngEncoder(PngEncoder::new(output.to_path_buf(), w, h))
    ));
    graph.add_edge(src, sink, EdgePorts::default());

    let pipeline = Pipeline::compile(&graph).map_err(|e| e.to_string())?;
    pipeline.run(None).map_err(|e| e.to_string())
}
```

---

## 5. ColorConvert (Real Implementation)

**Current**: `operation/color/cpu.rs` has `target: String` and a stub runner.
**Needed**: Actual color space conversion using the color model.

```rust
// pixors-executor/src/operation/color/cpu.rs (rewrite)

use crate::model::color::{ColorSpace, convert};
use crate::data::{Buffer, Tile};
// ...

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub source: ColorSpace,
    pub target: ColorSpace,
}

impl Stage for ColorConvert {
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(ColorConvertRunner {
            source: self.source,
            target: self.target,
        }))
    }
}

pub struct ColorConvertRunner {
    source: ColorSpace,
    target: ColorSpace,
}

impl CpuKernel for ColorConvertRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let tile = match item { Item::Tile(t) => t, _ => return Ok(()), };
        let data = tile.data.as_cpu_slice().ok_or_else(|| Error::internal("GPU tile"))?;

        // Convert each pixel
        let mut out = vec![0u8; data.len()];
        let pixels = data.len() / 4;
        for i in 0..pixels {
            let r = data[i * 4];
            let g = data[i * 4 + 1];
            let b = data[i * 4 + 2];
            let a = data[i * 4 + 3];

            // Unapply source transfer → linear
            let linear = self.source.to_linear(r, g, b);
            // Chromatic adaptation + matrix transform
            let converted = self.source.to(self.target, linear);
            // Apply target transfer
            let encoded = self.target.from_linear(converted);

            out[i * 4] = encoded.0;
            out[i * 4 + 1] = encoded.1;
            out[i * 4 + 2] = encoded.2;
            out[i * 4 + 3] = a;
        }

        emit.emit(Item::Tile(Tile::new(tile.coord, tile.meta, Buffer::cpu(out))));
        Ok(())
    }
}
```

---

## 6. General Pattern: Adding a New Operation

Every operation follows this structure:

### 1. Define the stage struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyOp {
    pub param: u32,
}
```

### 2. Implement `Stage`

```rust
impl Stage for MyOp {
    fn kind(&self) -> &'static str { "my_op" }
    fn ports(&self) -> &'static PortSpec { &MY_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: false }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(MyOpRunner { param: self.param }))
    }
    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        // Return Some(...) if GPU path exists
        None
    }
}
```

### 3. Implement `CpuKernel`

```rust
pub struct MyOpRunner { param: u32 }

impl CpuKernel for MyOpRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        // 1. Match Item variant (Tile, ScanLine, Neighborhood)
        // 2. Extract data from Buffer::Cpu
        // 3. Process
        // 4. emit.emit(Item::...)
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        Ok(()) // flush any remaining data
    }
}
```

### 4. Add to the enum

```rust
// mod.rs of the operation category
pub enum OperationNode {
    // ...
    MyOp(cpu::MyOp),
}
```

Update the `Stage` impl on `OperationNode` with a new match arm.

### 5. Desktop usage

```rust
let stage = graph.add_stage(StageNode::Operation(
    OperationNode::MyOp(MyOp { param: 42 })
));
```

---

## Summary

| Feature | Priority | Complexity |
|---------|----------|------------|
| Composition/Blend | High | Medium — multi-input staging |
| ColorConvert (real) | High | Low — use existing color model |
| Cache writer/reader | Medium | Low — filesystem I/O |
| TIFF write | Medium | Low |
| PNG export pipeline | Medium | Low |
| GPU kernel for blur | Done ✓ | — |
| GPU kernel for ColorConvert | Future | High — needs SPIR-V shader |
