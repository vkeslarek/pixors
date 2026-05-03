use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::runner::{OperationRunner, SinkRunner, SourceRunner};
use crate::pipeline::exec::*;
use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    Cpu,
    Gpu,
}

#[enum_dispatch]
pub trait Stage {
    fn kind(&self) -> &'static str;
    fn device(&self) -> Device;
    fn allocates_output(&self) -> bool;
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
pub enum ExecStage {
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
    DisplaySink,
}
