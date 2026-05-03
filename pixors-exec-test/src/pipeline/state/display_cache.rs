use serde::{Deserialize, Serialize};

use crate::pipeline::exec::Device;
use crate::pipeline::exec::ExecNode;
use crate::pipeline::exec;
use super::{ExpandCtx, ExpansionOption};
use crate::pipeline::state_graph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayCache {
    pub generation: u64,
}

impl super::StateNodeTrait for DisplayCache {
    fn kind(&self) -> &'static str {
        "display_cache"
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
            stages: vec![ExecNode::TileSink(exec::TileSink)],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
