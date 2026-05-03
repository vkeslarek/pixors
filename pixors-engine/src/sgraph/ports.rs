use serde::{Deserialize, Serialize};

/// Type carried by a port. Drives both validation (a `Layer` cannot connect
/// to an `Image` input) and runtime dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortType {
    /// Sentinel for "no value" — connections involving `Unit` skip type checking.
    Unit,
    Layers,
    Layer,
    Image,
    Mask,
    Histogram,
}

/// Declares one input or output of a node: its name, what it carries, and
/// whether the graph is invalid if the port is left unconnected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
    pub name: String,
    pub port_type: PortType,
    pub required: bool,
}

impl PortSpec {
    pub fn new(name: impl Into<String>, port_type: PortType) -> Self {
        Self {
            name: name.into(),
            port_type,
            required: true,
        }
    }

    /// Sink port for nodes that produce no downstream value (e.g. a display
    /// or file export). Marked optional since it never gets connected.
    pub fn unit() -> Self {
        Self {
            name: "output".into(),
            port_type: PortType::Unit,
            required: false,
        }
    }
}
