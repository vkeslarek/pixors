use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::Device;
use crate::pipeline::egraph::stage::ExecStage;
use crate::pipeline::exec;
use crate::pipeline::sgraph::node::{ExpandCtx, ExpansionOption, ExportFormat};
use crate::pipeline::sgraph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub path: PathBuf,
    pub format: ExportFormat,
}

impl crate::pipeline::sgraph::node::StateNodeTrait for Export {
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
                ExecStage::TileToScanline(exec::TileToScanline),
                ExecStage::PngEncoder(exec::PngEncoder {
                    path: self.path.clone(),
                }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
