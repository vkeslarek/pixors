use std::sync::{Arc, Mutex};

use pixors_executor::common::color::space::ColorSpace;
use pixors_executor::common::image::Image;
use pixors_executor::common::image::codec::EncoderConfig;
use pixors_executor::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_executor::data_transform::to_tile::ScanLineToTile;
use pixors_executor::operation::color::ColorConvert;
use pixors_executor::sink::SinkNode;
use pixors_executor::source::image_stream::ImageStreamSource;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::{EditorState, TabId};

const TILE_SIZE: u32 = 256;

#[derive(Debug)]
pub struct Export {
    pub tab: TabId,
    pub source_path: std::path::PathBuf,
    pub save_path: std::path::PathBuf,
    pub config: EncoderConfig,
    pub dpi: Option<pixors_executor::common::image::Dpi>,
    pub icc_profile: Option<Vec<u8>>,
    pub image_height: u32,
}

impl Action for Export {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        let img = Image::open(&self.source_path).map_err(|e| e.to_string())?;
        let w = img.desc.width;
        let h = img.desc.height;

        let stream = Arc::new(Mutex::new(Some(
            img.open_page(0).map_err(|e| e.to_string())?,
        )));

        let encoder_sink = match &self.config {
            EncoderConfig::Png(png_cfg) => {
                SinkNode::PngEncoderV2(pixors_executor::sink::png_encoder_v2::PngEncoderV2 {
                    path: self.save_path.clone(),
                    config: png_cfg.clone(),
                    dpi: self.dpi,
                    icc_profile: self.icc_profile.clone(),
                })
            }
            EncoderConfig::Tiff(tiff_cfg) => {
                SinkNode::TiffEncoder(pixors_executor::sink::tiff_encoder::TiffEncoderStage {
                    path: self.save_path.clone(),
                    config: tiff_cfg.clone(),
                    dpi: self.dpi,
                    icc_profile: self.icc_profile.clone(),
                })
            }
        };

        let graph = PathBuilder::new()
            .src(ImageStreamSource {
                stream,
                image_height: self.image_height,
            })
            .data_xform(ScanLineToTile {
                tile_size: TILE_SIZE,
                image_width: w,
                image_height: h,
            })
            .op(ColorConvert {
                target_format: PixelFormat::Rgba8,
                target_color_space: ColorSpace::SRGB,
                target_alpha: AlphaPolicy::Straight,
            })
            .sink(encoder_sink)
            .compile();

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Apply,
            graph,
            snapshot: None,
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
