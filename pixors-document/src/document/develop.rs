use serde::{Deserialize, Serialize};

use super::NodeId;
use super::transform::Operation;

/// Ordered chain of global adjustments applied to the primary asset
/// BEFORE the layer stack. Uses the same Operation type as transforms.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevelopState {
    pub adjustments: Vec<DevelopAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopAdjustment {
    pub id: NodeId,
    pub op: Operation,
    pub enabled: bool,
}
