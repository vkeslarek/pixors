use serde::{Deserialize, Serialize};

use super::{Device, Stage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_id: String,
}

impl Stage for CacheReader {
    fn kind(&self) -> &'static str {
        "cache_reader"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
}
