use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use crate::data::Device;
use crate::error::Error;
use crate::graph::runner::{OperationRunner, SinkRunner, SourceRunner};
use crate::operation::{
    BlurKernel, BlurKernelGpu, CacheReader, CacheWriter, ColorConvert, Download,
    FileDecoder, NeighborhoodAgg, PngEncoder, ScanLineAccumulator,
    TileToScanline, Upload,
};
use crate::sink::TileSink;

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
