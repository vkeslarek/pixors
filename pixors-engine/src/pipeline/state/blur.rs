use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::Device;
use crate::pipeline::egraph::stage::ExecStage;
use crate::pipeline::exec;
use crate::pipeline::sgraph::node::{ExpandCtx, ExpansionOption};
use crate::pipeline::sgraph::ports::{PortSpec, PortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blur {
    pub radius: u32,
}

impl crate::pipeline::sgraph::node::StateNodeTrait for Blur {
    fn kind(&self) -> &'static str {
        "blur"
    }
    fn inputs(&self) -> Vec<PortSpec> {
        vec![image_port("input")]
    }
    fn outputs(&self) -> Vec<PortSpec> {
        vec![image_port("output")]
    }
    fn expand(&self, ctx: &ExpandCtx) -> Vec<ExpansionOption> {
        let mut opts = vec![ExpansionOption {
            device: Device::Cpu,
            prefer: 1,
            stages: vec![
                ExecStage::NeighborhoodAgg(exec::NeighborhoodAgg {
                    radius: self.radius,
                }),
                ExecStage::BlurKernel(exec::BlurKernel {
                    radius: self.radius,
                }),
            ],
        }];
        if ctx.gpu_available {
            opts.insert(
                0,
                ExpansionOption {
                    device: Device::Gpu,
                    prefer: 100,
                    stages: vec![
                        ExecStage::NeighborhoodAgg(exec::NeighborhoodAgg {
                            radius: self.radius,
                        }),
                        ExecStage::BlurKernelGpu(exec::BlurKernelGpu {
                            radius: self.radius,
                        }),
                    ],
                },
            );
        }
        opts
    }
}

fn image_port(name: &str) -> PortSpec {
    PortSpec::new(name, PortType::Image)
}
