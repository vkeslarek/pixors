pub mod blur_kernel;
pub mod cache_reader;
pub mod cache_writer;
pub mod color_convert;
pub mod download;
pub mod file_decoder;
pub mod png_encoder;
pub mod tile_sink;
pub mod to_neighborhood;
pub mod to_scanline;
pub mod to_tile;
pub mod upload;

pub use blur_kernel::{BlurKernel, BlurKernelGpu, BlurKernelGpuRunner, BlurKernelRunner};
pub use cache_reader::CacheReader;
pub use cache_writer::CacheWriter;
pub use color_convert::ColorConvert;
pub use download::Download;
pub use file_decoder::FileDecoder;
pub use png_encoder::PngEncoder;
pub use tile_sink::{TileSink, install_tile_sink};
pub use to_neighborhood::NeighborhoodAgg;
pub use to_scanline::TileToScanline;
pub use to_tile::ScanLineAccumulator;
pub use upload::Upload;

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::pipeline::exec_graph::runner::{OperationRunner, SinkRunner, SourceRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    Source,
    Operation,
    Sink,
}

#[enum_dispatch]
pub trait Stage {
    fn kind(&self) -> &'static str;
    fn device(&self) -> Device;
    fn allocates_output(&self) -> bool;
    fn role(&self) -> StageRole {
        StageRole::Operation
    }
    fn source_runner(&self) -> Result<Box<dyn SourceRunner>, Error> {
        Err(Error::internal("not a source stage"))
    }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Err(Error::internal("not an operation stage"))
    }
    fn sink_runner(&self) -> Result<Box<dyn SinkRunner>, Error> {
        Err(Error::internal("not a sink stage"))
    }
}

#[enum_dispatch(Stage)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecNode {
    FileDecoder,
    ScanLineAccumulator,
    ColorConvert,
    NeighborhoodAgg,
    BlurKernel,
    BlurKernelGpu,
    Upload,
    Download,
    CacheReader,
    CacheWriter,
    PngEncoder,
    TileToScanline,
    TileSink,
}
