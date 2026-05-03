use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::{Device, Stage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWriter {
    pub cache_id: String,
}

impl Stage for CacheWriter {
    fn kind(&self) -> &'static str {
        "cache_writer"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
}
