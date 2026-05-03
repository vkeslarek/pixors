pub mod blur;
pub mod disk_cache;
pub mod display_cache;
pub mod export;
pub mod file_image;

pub use blur::Blur;
pub use disk_cache::DiskCache;
pub use display_cache::DisplayCache;
pub use export::Export;
pub use file_image::FileImage;

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::pipeline::exec::{Device, ExecNode};
use crate::pipeline::state_graph::ports::PortSpec;

pub struct ExpandCtx {
    pub gpu_available: bool,
}

impl ExpandCtx {
    pub fn cpu_only() -> Self {
        Self { gpu_available: false }
    }
}

pub struct ExpansionOption {
    pub stages: Vec<ExecNode>,
    pub device: Device,
    pub prefer: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

#[enum_dispatch]
pub trait StateNodeTrait {
    fn kind(&self) -> &'static str;
    fn inputs(&self) -> Vec<PortSpec>;
    fn outputs(&self) -> Vec<PortSpec>;
    fn expand(&self, ctx: &ExpandCtx) -> Vec<ExpansionOption>;
}

#[enum_dispatch(StateNodeTrait)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateNode {
    FileImage,
    Blur,
    DiskCache,
    DisplayCache,
    Export,
}

impl StateNode {
    pub fn serialize_params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}
