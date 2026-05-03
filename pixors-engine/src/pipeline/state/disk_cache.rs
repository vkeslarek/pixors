use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::Device;
use crate::pipeline::egraph::stage::ExecStage;
use crate::pipeline::exec;
use crate::pipeline::sgraph::node::{ExpandCtx, ExpansionOption};
use crate::pipeline::sgraph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskCache {
    pub cache_id: Option<String>,
}

impl crate::pipeline::sgraph::node::StateNodeTrait for DiskCache {
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
                ExecStage::CacheWriter(exec::CacheWriter {
                    cache_id: id.clone(),
                }),
                ExecStage::CacheReader(exec::CacheReader { cache_id: id }),
            ],
        }]
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
