use std::sync::{Arc, Mutex};

use pixors_color::processor::ColorConvert;
use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::data_transform::to_tile::ScanLineToTile;
use pixors_image::codec::EncoderConfig;
use pixors_image::image::open_image;
use pixors_image::source::image_stream::ImageStreamSource;

use crate::PathBuilder;
use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::{EditorState, TabId};

use crate::TILE_SIZE;

#[derive(Debug)]
pub struct Export {
    pub tab: TabId,
    pub source_path: std::path::PathBuf,
    pub save_path: std::path::PathBuf,
    pub config: EncoderConfig,
    pub dpi: Option<pixors_image::image::Dpi>,
    pub icc_profile: Option<Vec<u8>>,
    pub image_height: u32,
}

impl Action for Export {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        let img = open_image(&self.source_path).map_err(|e| e.to_string())?;
        let w = img.desc.width;
        let h = img.desc.height;

        let stream = Arc::new(Mutex::new(Some(
            img.open_page(0).map_err(|e| e.to_string())?,
        )));

        let encoder_sink: std::sync::Arc<dyn pixors_engine::stage::Stage + Send + Sync> =
            match &self.config {
                EncoderConfig::Png(png_cfg) => {
                    Arc::new(pixors_image::sink::png_encoder_v2::PngEncoderV2 {
                        path: self.save_path.clone(),
                        config: png_cfg.clone(),
                        dpi: self.dpi,
                        icc_profile: self.icc_profile.clone(),
                    })
                }
                EncoderConfig::Tiff(tiff_cfg) => {
                    Arc::new(pixors_image::sink::tiff_encoder::TiffEncoderStage {
                        path: self.save_path.clone(),
                        config: tiff_cfg.clone(),
                        dpi: self.dpi,
                        icc_profile: self.icc_profile.clone(),
                    })
                }
            };

        let graph = PathBuilder::new()
            .src(Arc::new(ImageStreamSource {
                stream,
                image_height: self.image_height,
            }))
            .data_xform(Arc::new(ScanLineToTile {
                tile_size: TILE_SIZE,
                image_width: w,
                image_height: h,
            }))
            .op(Arc::new(ColorConvert {
                target_format: PixelFormat::Rgba8,
                target_color_space: ColorSpace::SRGB,
                target_alpha: AlphaPolicy::Straight,
            }))
            .sink(encoder_sink)
            .compile();

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Apply,
            graph,
            routed_tab: None,
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.view.loading = false;
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
