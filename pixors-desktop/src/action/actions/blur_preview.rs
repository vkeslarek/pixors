use std::path::PathBuf;

use pixors_executor::common::color::space::ColorSpace;
use pixors_executor::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_executor::data_transform::to_neighborhood::TileToNeighborhood;
use pixors_executor::operation::blur::Blur;
use pixors_executor::operation::color::ColorConvert;
use pixors_executor::sink::viewport_cache_sink::ViewportCacheSink;
use pixors_executor::source::cache_reader::CacheReader;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::{EditorState, TabId};

const TILE_SIZE: u32 = 256;

#[derive(Debug)]
pub struct BlurPreview {
    pub tab: TabId,
    pub radius: u32,
    pub generation: u64,
    pub cache_dir: PathBuf,
    pub img_w: u32,
    pub img_h: u32,
    pub mip: u32,
    pub range: pixors_executor::source::cache_reader::TileRange,
}

impl Action for BlurPreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        // CacheReader reads ACEScg f16 tiles from disk (stored by CacheWriter during open).
        // TileToNeighborhood accumulates neighbour tiles for context-aware blur.
        // Blur works in ACEScg; convert to sRGB only for the viewport sink.
        let graph = PathBuilder::new()
            .src(CacheReader {
                cache_dir: self.cache_dir.clone(),
                mip_level: self.mip,
                tile_size: TILE_SIZE,
                image_width: self.img_w,
                image_height: self.img_h,
                tile_range: Some(self.range.clone()),
                pixel_format: PixelFormat::RgbaF16,
                color_space: ColorSpace::ACES_CG,
            })
            .data_xform(TileToNeighborhood {
                radius: self.radius,
            })
            .op(Blur {
                radius: self.radius,
            })
            .op(ColorConvert {
                target_format: PixelFormat::Rgba8,
                target_color_space: ColorSpace::SRGB,
                target_alpha: AlphaPolicy::Straight,
            })
            .sink(ViewportCacheSink::new(self.tab.0, self.generation))
            .compile();

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            snapshot: None,
            routed_tab: None,
        })
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
