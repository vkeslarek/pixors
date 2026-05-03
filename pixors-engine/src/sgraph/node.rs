use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::egraph::stage::ExecStage;
use crate::sgraph::ports::{PortSpec, PortType};

/// Default tile dimension used by the tile generator stage.
const DEFAULT_TILE_SIZE: u32 = 512;

/// Working color space the engine converts decoded inputs into.
const WORKING_COLOR_SPACE: &str = "ACEScg_f16";

/// Per-expansion context. Currently empty, but kept as a stable hook for
/// passing viewport/preview info to `expand` later.
pub struct ExpandCtx;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

/// User-facing node in the state graph (`sgraph`).
///
/// A `StateNode` is a high-level operation. Compilation calls `expand` to turn
/// each one into a sequence of low-level `ExecStage`s that the runner executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateNode {
    FileImage { path: PathBuf },
    Blur { radius: u32 },
    DiskCache { cache_id: Option<String> },
    DisplayCache { generation: u64 },
    Export { path: PathBuf, format: ExportFormat },
}

impl StateNode {
    /// Stable string identifier for the variant. Used for logging and tests.
    pub fn kind(&self) -> &'static str {
        match self {
            StateNode::FileImage { .. } => "file_image",
            StateNode::Blur { .. } => "blur",
            StateNode::DiskCache { .. } => "disk_cache",
            StateNode::DisplayCache { .. } => "display_cache",
            StateNode::Export { .. } => "export",
        }
    }

    pub fn inputs(&self) -> Vec<PortSpec> {
        match self {
            StateNode::FileImage { .. } => vec![],
            StateNode::Blur { .. }
            | StateNode::DiskCache { .. }
            | StateNode::DisplayCache { .. }
            | StateNode::Export { .. } => vec![image_port("input")],
        }
    }

    pub fn outputs(&self) -> Vec<PortSpec> {
        match self {
            StateNode::FileImage { .. } | StateNode::Blur { .. } | StateNode::DiskCache { .. } => {
                vec![image_port("output")]
            }
            // Sinks: nothing downstream consumes them.
            StateNode::DisplayCache { .. } | StateNode::Export { .. } => vec![],
        }
    }

    /// Lower this state node into the ordered list of execution stages that
    /// implement it. Each `StateNode` knows its own decomposition; the
    /// compiler just stitches the resulting stage chains together.
    pub fn expand(&self, _ctx: &ExpandCtx) -> Vec<ExecStage> {
        match self {
            StateNode::FileImage { path } => vec![
                ExecStage::FileDecoder { path: path.clone() },
                ExecStage::ScanLineAccumulator {
                    tile_size: DEFAULT_TILE_SIZE,
                },
                ExecStage::ColorConvert {
                    target: WORKING_COLOR_SPACE.into(),
                },
            ],
            StateNode::Blur { radius } => vec![
                ExecStage::NeighborhoodAgg { radius: *radius },
                ExecStage::BlurKernel { radius: *radius },
            ],
            StateNode::DiskCache { cache_id } => {
                let id = cache_id.clone().unwrap_or_default();
                vec![
                    ExecStage::CacheWriter {
                        cache_id: id.clone(),
                    },
                    ExecStage::CacheReader { cache_id: id },
                ]
            }
            StateNode::DisplayCache { .. } => vec![ExecStage::DisplaySink],
            StateNode::Export { path, format: _ } => {
                vec![
                    ExecStage::TileToScanline,
                    ExecStage::PngEncoder { path: path.clone() },
                ]
            }
        }
    }

    pub fn serialize_params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
