use serde::{Deserialize, Serialize};

use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::pixel::meta::PixelMeta;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

use crate::error::Error;

use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::data::tile::{Tile, TileCoord};
use crate::debug_stopwatch;

static SA_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "scanline",
    kind: DataKind::ScanLine,
}];

static SA_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static SA_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(SA_INPUTS),
    outputs: PortGroup::Fixed(SA_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanLineToTile {
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
}

impl Stage for ScanLineToTile {
    fn kind(&self) -> &'static str {
        "scanline_accumulator"
    }

    fn ports(&self) -> &'static PortSpecification {
        &SA_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn device(&self) -> Device {
        Device::Either
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(ScanLineToTileProcessor::new(self.tile_size)))
    }
    fn work_multiplier(&self) -> f64 {
        let cols = self.image_width.div_ceil(self.tile_size) as f64;
        let rows = self.image_height.div_ceil(self.tile_size) as f64;
        (cols * rows) / (self.image_height as f64).max(1.0)
    }
}

pub struct ScanLineToTileProcessor {
    tile_size: u32,
    rows: Vec<Vec<u8>>,
    meta: Option<PixelMeta>,
    mip_level: u32,
    band_y: u32,
    image_width: u32,
    image_height: u32,
    initialized: bool,
}

impl ScanLineToTileProcessor {
    pub fn new(tile_size: u32) -> Self {
        Self {
            tile_size,
            rows: vec![],
            meta: None,
            mip_level: 0,
            band_y: 0,
            image_width: 0,
            image_height: 0,
            initialized: false,
        }
    }
}

impl Processor for ScanLineToTileProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        ctx.ensure_cpu()?;
        let _sw = debug_stopwatch!("scanline_accumulator");
        let scanline = ProcessorContext::take_scanline(item)?;
        if !self.initialized {
            self.meta = Some(scanline.meta);
            self.mip_level = scanline.mip_level;
            self.image_width = scanline.width;
            self.initialized = true;
        }
        self.image_height = self.image_height.max(scanline.y + 1);
        self.rows.push(match &scanline.data {
            Buffer::Cpu(v) => (**v).clone(),
            Buffer::Gpu(_) => Vec::new(),
        });
        if self.rows.len() >= self.tile_size as usize {
            self.emit_tiles(ctx.emit);
        }
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        if !self.rows.is_empty() {
            self.emit_tiles(ctx.emit);
        }
        Ok(())
    }
}

impl ScanLineToTileProcessor {
    fn emit_tiles(&mut self, emit: &mut Emitter<Item>) {
        let meta = self.meta.unwrap();
        let rows = std::mem::take(&mut self.rows);
        let band_h = rows.len() as u32;
        let bpp = meta.format.bytes_per_pixel();
        let tiles_x = self.image_width.div_ceil(self.tile_size);

        for tx in 0..tiles_x {
            let px = tx * self.tile_size;
            let tw = ((self.image_width - px).min(self.tile_size)) as usize;
            let mut buf = Vec::with_capacity(tw * band_h as usize * bpp);
            for row in &rows {
                let s = (px as usize * bpp).min(row.len());
                let e = (s + tw * bpp).min(row.len());
                buf.extend_from_slice(&row[s..e]);
            }
            let coord = TileCoord::new(
                self.mip_level,
                tx,
                self.band_y,
                self.tile_size,
                self.image_width,
                self.image_height,
            );
            emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(buf))));
        }
        self.band_y += 1;
    }
}
