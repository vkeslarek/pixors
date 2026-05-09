use std::sync::Arc;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::graph::path::Path;
use pixors_color::operation::color::ColorConvert;
use crate::tile_cache_sink::TileCacheSink;
use pixors_ops::source::cache_reader::{CacheReader, TileRange};

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::PathBuilder;
use crate::state::{EditorState, TabId};

use crate::TILE_SIZE;

#[derive(Debug)]
pub struct RequestMipFetch {
    pub tab: TabId,
    pub mip: u32,
    pub range: TileRange,
    pub cache_dir: std::path::PathBuf,
    pub img_w: u32,
    pub img_h: u32,
    pub post_process: Option<Path>,
}

impl Action for RequestMipFetch {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        let mut pipe = PathBuilder::new()
            .src(Arc::new(CacheReader {
                cache_dir: self.cache_dir.clone(),
                mip_level: self.mip,
                tile_size: TILE_SIZE,
                image_width: self.img_w,
                image_height: self.img_h,
                tile_range: Some(self.range.clone()),
                pixel_format: PixelFormat::RgbaF16,
                color_space: ColorSpace::ACES_CG,
            }));

        if let Some(ref path) = self.post_process {
            pipe = pipe.attach(path);
        } else {
            pipe = pipe.op(Arc::new(ColorConvert {
                target_format: PixelFormat::Rgba8,
                target_color_space: ColorSpace::SRGB,
                target_alpha: AlphaPolicy::Straight,
            }));
        }

        let graph = pipe
            .sink(Arc::new(TileCacheSink::new(self.tab.0, 0)))
            .compile();

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            routed_tab: None,
        })
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
