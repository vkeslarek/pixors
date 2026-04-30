pub mod session;
pub mod tab;
pub mod layer;
pub mod loader;
pub mod tool;
pub mod viewport;
pub mod working_image;
pub mod filter;

use async_trait::async_trait;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;
use crate::server::app::AppState;
use crate::server::ws::types::ConnectionContext;

/// Every feature module in the engine is a [`Service`].
///
/// Each service defines its own `Command` and `Event` enums, and implements `handle_command`.
/// The central [`EngineCommand`]/[`EngineEvent`] enums in `event_bus.rs` wrap these per‑service
/// types so that serde can deserialize incoming messages. The `route_command` match in `app.rs`
/// dispatches each variant to the concrete service's `handle_command`.
#[async_trait]
pub trait Service: Send + Sync + 'static {
    type Command: Debug + Clone + serde::Serialize + serde::de::DeserializeOwned + Send + 'static;
    type Event: Debug + Clone + Serialize + Send + 'static;

    async fn handle_command(
        &self,
        cmd: Self::Command,
        state: &Arc<AppState>,
        ctx: &mut ConnectionContext,
    );
}
