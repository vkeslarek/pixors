use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::pipeline::exec::Device;
use crate::pipeline::exec::ExecNode;
use crate::pipeline::exec;
use super::{ExpandCtx, ExpansionOption};
use crate::pipeline::state_graph::ports::{PortSpec, PortType};

const DEFAULT_TILE_SIZE: u32 = 512;
const WORKING_COLOR_SPACE: &str = "ACEScg_f16";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileImage {
    pub path: PathBuf,
}

impl super::StateNodeTrait for FileImage {
    fn kind(&self) -> &'static str {
        "file_image"
    }
    fn inputs(&self) -> Vec<PortSpec> {
        vec![]
    }
    fn outputs(&self) -> Vec<PortSpec> {
        vec![image_port("output")]
    }
    fn expand(&self, _ctx: &ExpandCtx) -> Vec<ExpansionOption> {
        vec![ExpansionOption {
            device: Device::Cpu,
            prefer: 1,
            stages: vec![
                ExecNode::FileDecoder(exec::FileDecoder {
                    path: self.path.clone(),
                }),
                ExecNode::ScanLineAccumulator(exec::ScanLineAccumulator {
                    tile_size: DEFAULT_TILE_SIZE,
                }),
                ExecNode::ColorConvert(exec::ColorConvert {
                    target: WORKING_COLOR_SPACE.into(),
                }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
