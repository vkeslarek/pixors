use serde::{Deserialize, Serialize};

/// The execution device (CPU/GPU) assigned to a pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    /// Runs on CPU, requires CPU input data (Download inserted before if GPU upstream).
    Cpu,
    /// Runs on GPU, requires GPU input data (Upload inserted before if CPU upstream).
    Gpu,
    /// Runs the CPU kernel but accepts input data from either device.
    /// No Upload/Download is inserted on edges feeding into this stage.
    Either,
}
