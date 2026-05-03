use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::{Device, Stage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySink;

impl Stage for DisplaySink {
    fn kind(&self) -> &'static str {
        "display_sink"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
}
