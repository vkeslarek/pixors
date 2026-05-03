use serde::{Deserialize, Serialize};

use super::{Device, Stage, StageRole};

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
    fn role(&self) -> StageRole {
        StageRole::Sink
    }
}
