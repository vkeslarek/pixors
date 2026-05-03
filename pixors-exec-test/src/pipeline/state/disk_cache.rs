use serde::{Deserialize, Serialize};

use crate::pipeline::exec::Device;
use crate::pipeline::exec::ExecNode;
use crate::pipeline::exec;
use super::{ExpandCtx, ExpansionOption};
use crate::pipeline::state_graph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskCache {
    pub cache_id: Option<String>,
}

impl super::StateNodeTrait for DiskCache {
    fn kind(&self) -> &'static str {
        "disk_cache"
    }
    fn inputs(&self) -> Vec<PortSpec> {
        vec![image_port("input")]
    }
    fn outputs(&self) -> Vec<PortSpec> {
        vec![image_port("output")]
    }
    fn expand(&self, _ctx: &ExpandCtx) -> Vec<ExpansionOption> {
        let id = self.cache_id.clone().unwrap_or_default();
        vec![ExpansionOption {
            device: Device::Cpu,
            prefer: 1,
            stages: vec![
                ExecNode::CacheWriter(exec::CacheWriter {
                    cache_id: id.clone(),
                }),
                ExecNode::CacheReader(exec::CacheReader { cache_id: id }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
