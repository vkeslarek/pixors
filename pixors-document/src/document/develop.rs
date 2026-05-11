use serde::{Deserialize, Serialize};

use super::NodeId;

/// Ordered chain of global adjustments applied to the primary asset
/// BEFORE the layer stack. Empty in Phase 10 — slot for future.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevelopState {
    pub adjustments: Vec<DevelopAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopAdjustment {
    pub id: NodeId,
    pub adjustment: Adjustment,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Adjustment {
    Blur { radius: f32 },
    Exposure { ev: f32 },
}
