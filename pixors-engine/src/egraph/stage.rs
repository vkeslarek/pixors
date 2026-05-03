use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::egraph::runner::{OperationRunner, SinkRunner, SourceRunner};
use crate::egraph::stages;
use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecStage {
    FileDecoder { path: PathBuf },
    ScanLineAccumulator { tile_size: u32 },
    ColorConvert { target: String },
    NeighborhoodAgg { radius: u32 },
    BlurKernel { radius: u32 },
    CacheReader { cache_id: String },
    CacheWriter { cache_id: String },
    PngEncoder { path: PathBuf },
    TileToScanline,
    DisplaySink,
}

impl ExecStage {
    pub fn kind(&self) -> &'static str {
        match self {
            ExecStage::FileDecoder { .. } => "file_decoder",
            ExecStage::ScanLineAccumulator { .. } => "scanline_accumulator",
            ExecStage::ColorConvert { .. } => "color_convert",
            ExecStage::NeighborhoodAgg { .. } => "neighborhood_agg",
            ExecStage::BlurKernel { .. } => "blur_kernel",
            ExecStage::CacheReader { .. } => "cache_reader",
            ExecStage::CacheWriter { .. } => "cache_writer",
            ExecStage::PngEncoder { .. } => "png_encoder",
            ExecStage::TileToScanline => "tile_to_scanline",
            ExecStage::DisplaySink => "display_sink",
        }
    }

    pub fn source_runner(&self) -> Result<Box<dyn SourceRunner>, Error> {
        match self {
            ExecStage::FileDecoder { path } => Ok(Box::new(
                stages::file_decoder::FileDecoder::new(path.clone()),
            )),
            _ => Err(Error::internal("not a source stage")),
        }
    }

    pub fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        match self {
            ExecStage::ScanLineAccumulator { tile_size } => Ok(Box::new(
                stages::scanline_accumulator::ScanLineAccumulatorRunner::new(*tile_size),
            )),
            ExecStage::ColorConvert { .. } => {
                Ok(Box::new(stages::color_convert::ColorConvertRunner))
            }
            ExecStage::NeighborhoodAgg { radius } => Ok(Box::new(
                stages::neighborhood_agg::NeighborhoodAggRunner::new(*radius),
            )),
            ExecStage::BlurKernel { radius } => Ok(Box::new(
                stages::blur_kernel::BlurKernelRunner::new(*radius),
            )),
            ExecStage::TileToScanline => Ok(Box::new(
                stages::tile_to_scanline::TileToScanlineRunner::new(),
            )),
            _ => Err(Error::internal("not an operation stage")),
        }
    }

    pub fn sink_runner(&self) -> Result<Box<dyn SinkRunner>, Error> {
        match self {
            ExecStage::PngEncoder { path } => Ok(Box::new(
                stages::png_encoder::PngEncoderRunner::new(path.clone()),
            )),
            _ => Err(Error::internal("not a sink stage")),
        }
    }
}
