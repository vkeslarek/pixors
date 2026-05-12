use serde::{Deserialize, Serialize};

/// Unique identifier for an editing session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);
