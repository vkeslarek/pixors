use std::sync::{Arc, Mutex};

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::tile::Tile;
use pixors_engine::data::tile::TileCoord;
use pixors_engine::data_transform::to_neighborhood::TileToNeighborhood;
use pixors_engine::graph::item::Item;
use pixors_ops::operation::blur::Blur;
use pixors_color::operation::color::ColorConvert;
use crate::tile_cache_sink::TileCacheSink;
use crate::tile_cache_source::{
    TileCacheSource, install_tile_cache_reader, is_tile_cache_reader_installed,
};

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::{EditorState, TabId};
use crate::viewport::tile_cache::TileCache;

const TILE_SIZE: u32 = 256;

#[derive(Debug)]
pub struct BlurPreview {
    pub tab: TabId,
    pub radius: u32,
    pub generation: u64,
    pub mip: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub cache: Arc<Mutex<TileCache>>,
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
}

impl Action for BlurPreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        if !is_tile_cache_reader_installed(self.tab.0) {
            let cache = self.cache.clone();
            let image_width = self.image_width;
            let image_height = self.image_height;
            let display_format = self.display_format;
            let display_color_space = self.display_color_space;

            install_tile_cache_reader(
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
        }

        let graph = PathBuilder::new()
            .src(Arc::new(TileCacheSource {
                routing_key: self.tab.0,
                mip_level: self.mip,
                generation: 0,
                tile_range: None,
            }))
            .op(Arc::new(ColorConvert {
                target_format: self.working_format,
                target_color_space: self.working_color_space,
                target_alpha: AlphaPolicy::Straight,
            }))
            .data_xform(Arc::new(TileToNeighborhood {
                radius: self.radius,
            }))
            .op(Arc::new(Blur {
                radius: self.radius,
            }))
            .op(Arc::new(ColorConvert {
                target_format: self.display_format,
                target_color_space: self.display_color_space,
                target_alpha: AlphaPolicy::Straight,
            }))
            .sink(Arc::new(TileCacheSink::new(self.tab.0, self.generation)))
            .compile();

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            routed_tab: None,
        })
    }

    fn apply(&self, _state: &mut EditorState, status: PipelineStatus) {
        if matches!(status, PipelineStatus::Cancelled | PipelineStatus::Error(_)) {
            if let Ok(mut guard) = self.cache.lock() {
                guard.clear_generation(self.generation);
            }
        }
    }

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
