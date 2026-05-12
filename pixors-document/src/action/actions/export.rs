use pixors_engine::cache::cache_reader::TileRange;
use pixors_engine::stage::Stage;
use pixors_image::codec::EncoderConfig;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::render::compiler::{CompileConfig, RenderRequest, compile};
use crate::{EditorState, SessionId};

use crate::TILE_SIZE;

#[derive(Debug)]
pub struct Export {
    pub tab: SessionId,
    pub save_path: std::path::PathBuf,
    pub config: EncoderConfig,
    pub dpi: Option<pixors_image::image::Dpi>,
    pub icc_profile: Option<Vec<u8>>,
}

impl Action for Export {
    fn target_tab(&self) -> Option<SessionId> {
        Some(self.tab)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let tab = state.session(self.tab).ok_or("tab not found")?;
        let img_w = tab.document.canvas.width;
        let img_h = tab.document.canvas.height;

        let encoder_sink = match &self.config {
            EncoderConfig::Png(png_cfg) => Stage::Consumer(Box::new(
                pixors_image::sink::png_encoder_v2::PngEncoderV2::new(
                    self.save_path.clone(),
                    png_cfg.clone(),
                    self.dpi,
                    self.icc_profile.clone(),
                ),
            )),
            EncoderConfig::Tiff(tiff_cfg) => Stage::Consumer(Box::new(
                pixors_image::sink::tiff_encoder::TiffEncoderStage::new(
                    self.save_path.clone(),
                    tiff_cfg.clone(),
                    self.dpi,
                    self.icc_profile.clone(),
                ),
            )),
        };

        let config = CompileConfig {
            disk_caches: tab.transient.disk_caches.clone(),
            cache_dir: tab.transient.cache_dir.clone(),
            display_format: tab.display_format,
            display_color_space: tab.display_color_space,
            working_format: tab.working_format,
            working_color_space: tab.working_color_space,
            tile_size: TILE_SIZE,
            img_w,
            img_h,
        };

        let ntx = img_w.div_ceil(TILE_SIZE);
        let nty = img_h.div_ceil(TILE_SIZE);
        let req = RenderRequest {
            viewport: TileRange {
                tx_start: 0,
                tx_end: ntx,
                ty_start: 0,
                ty_end: nty,
            },
            mip_level: 0,
            up_to: None,
        };

        let graph = compile(&tab.document, &req, &config, encoder_sink);

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Apply,
            graph,
            routed_tab: Some(self.tab),
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        if let Some(tab) = state.session_mut(self.tab) {
            tab.transient.view.loading = false;
        }
        match status {
            PipelineStatus::Done => {
                tracing::info!("[pixors] export complete: {}", self.save_path.display());
            }
            PipelineStatus::Error(e) => {
                tracing::error!("[pixors] export failed: {e}");
            }
            _ => {}
        }
    }

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
