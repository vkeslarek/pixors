use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::{BlendSpec, NodeId};

/// A single non-destructive transformation in an ordered stack.
///
/// Unifies what Photoshop splits into three concepts:
/// - Smart Filters  → `Transform { input: Layer,  output: Replace }`
/// - Adjustment Layers → `Transform { input: Below,  output: Replace }`
/// - Layer Effects  → `Transform { input: Layer,  output: Composite }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub id: NodeId,
    pub op: Operation,
    pub input: InputScope,
    pub output: OutputMode,
    pub enabled: bool,
}

/// Where a Transform reads its pixel input from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputScope {
    /// Pixels produced by the host layer (or the output of the previous Transform).
    Layer,
    /// The composited result of everything below this layer in the stack.
    Below,
    /// The pixel output of another node, referenced by id.
    Reference(NodeId),
}

/// How a Transform's output is incorporated back into the layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputMode {
    /// Output replaces (or alpha-blends over) the input.
    Replace { blend: BlendSpec },
    /// Output is composited alongside the original pixels (drop shadow, outer glow, etc.).
    Composite {
        blend: BlendSpec,
        position: CompositePosition,
    },
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Replace {
            blend: BlendSpec::default(),
        }
    }
}

/// Spatial relationship when `OutputMode::Composite`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompositePosition {
    Behind,
    InFront,
    Around,
}

/// User-visible parameterized operations.
///
/// Each variant maps 1-to-1 to a pipeline `Stage` that the render compiler emits.
/// Data-shape adapters (TileToNeighborhood, Upload/Download) are compiler internals, not Operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Blur { radius: f32 },
    Exposure { stops: f32 },
    // Future: Levels, Curves, HueSaturation, DropShadow, …
}

impl Operation {
    pub fn label(&self) -> &'static str {
        match self {
            Operation::Blur { .. } => "Blur",
            Operation::Exposure { .. } => "Exposure",
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            Operation::Blur { radius } => format!("radius {:.0}px", radius),
            Operation::Exposure { stops } => format!("{:+.1} stops", stops),
        }
    }

    pub fn color(&self) -> (f32, f32, f32) {
        match self {
            Operation::Blur { .. } => (0.5, 0.4, 0.7),
            Operation::Exposure { .. } => (0.8, 0.7, 0.3),
        }
    }

    /// Stable hash of the operation's parameters, used as part of a render cache key.
    pub fn params_hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash_params(&mut h);
        h.finish()
    }

    fn hash_params(&self, h: &mut impl Hasher) {
        std::mem::discriminant(self).hash(h);
        match self {
            Operation::Blur { radius } => radius.to_bits().hash(h),
            Operation::Exposure { stops } => stops.to_bits().hash(h),
        }
    }
}
