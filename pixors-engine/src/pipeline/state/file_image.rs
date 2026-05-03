use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::Device;
use crate::pipeline::egraph::stage::ExecStage;
use crate::pipeline::exec;
use crate::pipeline::sgraph::node::{ExpandCtx, ExpansionOption};
use crate::pipeline::sgraph::ports::{PortSpec, PortType};

const DEFAULT_TILE_SIZE: u32 = 512;
const WORKING_COLOR_SPACE: &str = "ACEScg_f16";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileImage {
    pub path: PathBuf,
}

impl crate::pipeline::sgraph::node::StateNodeTrait for FileImage {
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
                ExecStage::FileDecoder(exec::FileDecoder {
                    path: self.path.clone(),
                }),
                ExecStage::ScanLineAccumulator(exec::ScanLineAccumulator {
                    tile_size: DEFAULT_TILE_SIZE,
                }),
                ExecStage::ColorConvert(exec::ColorConvert {
                    target: WORKING_COLOR_SPACE.into(),
                }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
