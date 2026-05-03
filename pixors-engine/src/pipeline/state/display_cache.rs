use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::Device;
use crate::pipeline::egraph::stage::ExecStage;
use crate::pipeline::exec;
use crate::pipeline::sgraph::node::{ExpandCtx, ExpansionOption};
use crate::pipeline::sgraph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayCache {
    pub generation: u64,
}

impl crate::pipeline::sgraph::node::StateNodeTrait for DisplayCache {
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
            stages: vec![ExecStage::DisplaySink(exec::DisplaySink)],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
