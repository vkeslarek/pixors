use std::collections::BTreeMap;

use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::data::scanline::ScanLine;
use crate::data::tile::Tile;
use crate::graph::item::Item;
use crate::model::pixel::meta::PixelMeta;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};
use serde::{Deserialize, Serialize};

use crate::error::Error;

use crate::debug_stopwatch;

static TS_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static TS_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "scanline",
    kind: DataKind::ScanLine,
}];

static TS_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(TS_INPUTS),
    outputs: PortGroup::Fixed(TS_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileToScanline;

impl Stage for TileToScanline {
    fn kind(&self) -> &'static str {
        "tile_to_scanline"
    }

    fn ports(&self) -> &'static PortSpecification {
        &TS_PORTS
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
        Some(Box::new(TileToScanlineProcessor::new()))
    }
}

/// Inverse of `ScanLineToTile`: takes Tiles back to ScanLines.
///
/// Tiles can arrive in any order — upstream stages (e.g. `TileToNeighborhood`)
/// drain a `HashMap`, so band order is not guaranteed. We bucket tiles by
/// band (`ty`) and emit each band's scanlines on `finish` once the full
/// image is in. Per-band flushing on a `ty` change would corrupt rows when
/// columns from the same band are interleaved with another band.
pub struct TileToScanlineProcessor {
    bands: BTreeMap<u32, Vec<Tile>>,
    image_width: u32,
    mip_level: u32,
    meta: Option<PixelMeta>,
    initialized: bool,
}

impl TileToScanlineProcessor {
    pub fn new() -> Self {
        Self {
            bands: BTreeMap::new(),
            image_width: 0,
            mip_level: 0,
            meta: None,
            initialized: false,
        }
    }
}

impl Processor for TileToScanlineProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        ctx.ensure_cpu()?;
        let _sw = debug_stopwatch!("tile_to_scanline");
        let tile = ProcessorContext::take_tile(item)?;

        if !self.initialized {
            self.meta = Some(tile.meta);
            self.mip_level = tile.coord.mip_level;
            self.initialized = true;
        }
        self.image_width = self.image_width.max(tile.coord.px + tile.coord.width);

        self.bands.entry(tile.coord.ty).or_default().push(tile);
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let Some(meta) = self.meta else { return Ok(()) };
        let bpp = meta.format.bytes_per_pixel();
        let row_bytes = self.image_width as usize * bpp;

        // Iterate bands in ascending `ty`.
        let bands = std::mem::take(&mut self.bands);
        for (_ty, mut tiles) in bands {
            // Sort columns left-to-right within the band.
            tiles.sort_by_key(|t| t.coord.px);

            let band_py = tiles[0].coord.py;
            let band_h = tiles.iter().map(|t| t.coord.height).max().unwrap_or(0);

            for row in 0..band_h {
                let mut full_row = vec![0u8; row_bytes];
                for tile in &tiles {
                    if row >= tile.coord.height {
                        continue;
                    }
                    let data: &[u8] = match &tile.data {
                        Buffer::Cpu(v) => v.as_slice(),
                        Buffer::Gpu(_) => &[],
                    };
                    let tw = tile.coord.width as usize;
                    let src_off = row as usize * tw * bpp;
                    let dst_off = tile.coord.px as usize * bpp;
                    let len = (tw * bpp)
                        .min(data.len() - src_off)
                        .min(full_row.len() - dst_off);
                    full_row[dst_off..dst_off + len].copy_from_slice(&data[src_off..src_off + len]);
                }
                ctx.emit.emit(Item::ScanLine(ScanLine::new(
                    self.mip_level,
                    band_py + row,
                    self.image_width,
                    meta,
                    Buffer::cpu(full_row),
                )));
            }
        }
        Ok(())
    }
}
