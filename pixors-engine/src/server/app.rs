//! Application composition root — AppState and command routing.

use crate::color::ColorSpace;
use crate::server::event_bus::{EngineCommand, EventBus};
use crate::server::service::system::SystemService;
use crate::server::service::tab::TabService;
use crate::server::service::tool::ToolService;
use crate::server::service::viewport::ViewportService;
use crate::server::ws::types::ConnectionContext;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Conversion matrices
// ---------------------------------------------------------------------------

/// Pre-computed color conversion matrices for common transformations.
#[derive(Debug, Clone)]
pub struct ConversionMatrices {
    /// sRGB → ACEScg conversion matrix (3×3).
    #[allow(dead_code)]
    pub srgb_to_acescg: [[f32; 3]; 3],
    /// ACEScg → sRGB conversion matrix (3×3).
    #[allow(dead_code)]
    pub acescg_to_srgb: [[f32; 3]; 3],
}

impl ConversionMatrices {
    pub fn new() -> Self {
        let fwd = ColorSpace::SRGB
            .converter_to(ColorSpace::ACES_CG)
            .expect("sRGB → ACEScg conversion always valid");
        let rev = ColorSpace::ACES_CG
            .converter_to(ColorSpace::SRGB)
            .expect("ACEScg → sRGB conversion always valid");

        Self {
            srgb_to_acescg: fwd.matrix().as_3x3_array(),
            acescg_to_srgb: rev.matrix().as_3x3_array(),
        }
    }
}

impl Default for ConversionMatrices {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Global application state — a container of service Arc's.
/// Controllers and WebSocket handlers access fields directly.
#[derive(Debug, Clone)]
pub struct AppState {
    pub tab_service: Arc<TabService>,
    pub tool_service: Arc<ToolService>,
    pub viewport_service: Arc<ViewportService>,
    pub system_service: Arc<SystemService>,
    pub event_bus: Arc<EventBus>,
    #[allow(dead_code)]
    pub conv_matrices: ConversionMatrices,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            event_bus: EventBus::new(),
            tab_service: Arc::new(TabService::new(256)),
            tool_service: Arc::new(ToolService::new()),
            viewport_service: Arc::new(ViewportService::new()),
            system_service: Arc::new(SystemService::new()),
            conv_matrices: ConversionMatrices::new(),
        }
    }

    /// Dispatch a decoded command to the correct service.
    /// Add new services here — this is the only place routing lives.
    pub async fn route_command(&self, cmd: EngineCommand, ctx: &mut ConnectionContext) {
        let state = Arc::new(self.clone());
        match cmd {
            EngineCommand::Tab(c) => {
                self.tab_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::Viewport(c) => {
                self.viewport_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::Tool(c) => {
                self.tool_service.handle_command(c, &state, ctx).await;
            }
            EngineCommand::System(c) => {
                self.system_service.handle_command(c, &state, ctx).await;
            }
        }
    }
}

