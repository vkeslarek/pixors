use crate::data::device::Device;
use crate::error::Error;
use crate::gpu::context::GpuContext;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Context passed to every Producer / Processor invocation.
pub struct ProcessorContext<'a> {
    pub port: u16,
    pub device: Device,
    pub emit: &'a mut crate::graph::emitter::Emitter<crate::graph::item::Item>,
    pub gpu: Option<Arc<GpuContext>>,
    pub cancelled: Arc<AtomicBool>,
}

impl<'a> ProcessorContext<'a> {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn ensure_cpu(&self) -> Result<(), Error> {
        if self.device == Device::Cpu {
            Ok(())
        } else {
            Err(Error::internal(format!(
                "expected CPU, got {:?}",
                self.device
            )))
        }
    }

    pub fn ensure_gpu(&self) -> Result<(), Error> {
        if self.device == Device::Gpu {
            Ok(())
        } else {
            Err(Error::internal(format!(
                "expected GPU, got {:?}",
                self.device
            )))
        }
    }

    pub fn take_tile(item: crate::graph::item::Item) -> Result<crate::data::tile::Tile, Error> {
        match item {
            crate::graph::item::Item::Tile(t) => Ok(t),
            other => Err(Error::internal(format!(
                "expected Tile, got {:?}",
                other.kind()
            ))),
        }
    }

    pub fn take_scanline(
        item: crate::graph::item::Item,
    ) -> Result<crate::data::scanline::ScanLine, Error> {
        match item {
            crate::graph::item::Item::ScanLine(s) => Ok(s),
            other => Err(Error::internal(format!(
                "expected ScanLine, got {:?}",
                other.kind()
            ))),
        }
    }

    pub fn take_neighborhood(
        item: crate::graph::item::Item,
    ) -> Result<crate::data::neighborhood::Neighborhood, Error> {
        match item {
            crate::graph::item::Item::Neighborhood(n) => Ok(n),
            other => Err(Error::internal(format!(
                "expected Neighborhood, got {:?}",
                other.kind()
            ))),
        }
    }

    pub fn take_tile_block(
        item: crate::graph::item::Item,
    ) -> Result<crate::data::tile_block::TileBlock, Error> {
        match item {
            crate::graph::item::Item::TileBlock(b) => Ok(b),
            other => Err(Error::internal(format!(
                "expected TileBlock, got {:?}",
                other.kind()
            ))),
        }
    }
}
