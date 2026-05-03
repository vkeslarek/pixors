use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::pipeline::exec::Device;
use crate::pipeline::exec::ExecNode;
use crate::pipeline::exec;
use super::{ExpandCtx, ExpansionOption, ExportFormat};
use crate::pipeline::state_graph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub path: PathBuf,
    pub format: ExportFormat,
}

impl super::StateNodeTrait for Export {
    fn kind(&self) -> &'static str {
        "export"
    }
    fn inputs(&self) -> Vec<PortSpec> {
        vec![image_port("input")]
    }
    fn outputs(&self) -> Vec<PortSpec> {
        vec![]
    }
    fn expand(&self, _ctx: &ExpandCtx) -> Vec<ExpansionOption> {
        vec![ExpansionOption {
            device: Device::Cpu,
            prefer: 1,
            stages: vec![
                ExecNode::TileToScanline(exec::TileToScanline),
                ExecNode::PngEncoder(exec::PngEncoder {
                    path: self.path.clone(),
                }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
