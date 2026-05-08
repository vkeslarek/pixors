use std::sync::{Arc, Mutex};

use pixors_executor::common::color::space::ColorSpace;
use pixors_executor::common::pixel::meta::PixelMeta;
use pixors_executor::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_executor::data::buffer::Buffer;
use pixors_executor::data::tile::Tile;
use pixors_executor::data::tile::TileCoord;
use pixors_executor::data_transform::to_neighborhood::TileToNeighborhood;
use pixors_executor::graph::item::Item;
use pixors_executor::operation::blur::Blur;
use pixors_executor::operation::color::ColorConvert;
use pixors_executor::sink::viewport_cache_sink::ViewportCacheSink;
use pixors_executor::source::viewport_cache_source::{
    ViewportCacheSource, install_viewport_cache_reader,
};

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::{EditorState, TabId};
use crate::viewport::tile_cache::ViewportCache;

const TILE_SIZE: u32 = 256;

#[derive(Debug)]
pub struct BlurPreview {
    pub tab: TabId,
    pub radius: u32,
    pub generation: u64,
    pub mip: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub cache: Arc<Mutex<ViewportCache>>,
    /// Format/colorspace of tiles stored in the RAM cache (display space, e.g. sRGB Rgba8).
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
    /// Working format/colorspace for blur (linear, e.g. ACEScg RgbaF16).
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
}

impl Action for BlurPreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        let cache = self.cache.clone();
        let image_width = self.image_width;
        let image_height = self.image_height;
        let display_format = self.display_format;
        let display_color_space = self.display_color_space;

        // Register the RAM-cache reader for this tab. Tiles are in display space
        // (e.g. sRGB Rgba8) — label them honestly so downstream ColorConvert is correct.
        install_viewport_cache_reader(
            self.tab.0,
            Box::new(move |_key, generation, mip, _range| {
                let guard = cache.lock().unwrap();
                guard
                    .tiles_at_mip(mip, generation)
                    .into_iter()
                    .map(|(pos, ct)| {
                        Item::Tile(Tile::new(
                            TileCoord {
                                mip_level: pos.mip_level,
                                tx: pos.tx,
                                ty: pos.ty,
                                px: ct.px,
                                py: ct.py,
                                width: ct.width,
                                height: ct.height,
                                tile_size: TILE_SIZE,
                                image_width,
                                image_height,
                            },
                            PixelMeta::new(
                                display_format,
                                display_color_space,
                                AlphaPolicy::Straight,
                            ),
                            Buffer::cpu(ct.bytes.as_ref().clone()),
                        ))
                    })
                    .collect()
            }),
        );

        // Pipeline: decode display→working, blur in linear space, re-encode to display.
        let graph = PathBuilder::new()
            .src(ViewportCacheSource {
                routing_key: self.tab.0,
                mip_level: self.mip,
                generation: 0,
                tile_range: None,
            })
            .op(ColorConvert {
                target_format: self.working_format,
                target_color_space: self.working_color_space,
                target_alpha: AlphaPolicy::Straight,
            })
            .data_xform(TileToNeighborhood {
                radius: self.radius,
            })
            .op(Blur {
                radius: self.radius,
            })
            .op(ColorConvert {
                target_format: self.display_format,
                target_color_space: self.display_color_space,
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
