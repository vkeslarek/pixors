use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pixors_executor::common::color::space::ColorSpace;
use pixors_executor::common::image::Image;
use pixors_executor::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_executor::data::tile::TileGridPos;
use pixors_executor::data_transform::to_tile::ScanLineToTile;
use pixors_executor::operation::color::ColorConvert;
use pixors_executor::operation::mip_downsample::MipDownsample;
use pixors_executor::sink::cache_writer::CacheWriter;
use pixors_executor::sink::viewport_cache_sink::{
    install_viewport_cache_sink, ViewportCacheSink,
};
use pixors_executor::source::image_stream::ImageStreamSource;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::{EditorState, Tab, TabId, TabSource, TabView};
use crate::viewport::state::ViewportState;
use crate::viewport::tile_cache::{CachedTile, ViewportCache};

const TILE_SIZE: u32 = 256;

#[derive(Debug)]
pub struct OpenFile {
    pub path: std::path::PathBuf,
    pending_tab_id: Mutex<Option<TabId>>,
}

impl OpenFile {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            pending_tab_id: Mutex::new(None),
        }
    }
}

impl Action for OpenFile {
    fn target_tab(&self) -> Option<TabId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let img = Image::open(&self.path).map_err(|e| e.to_string())?;
        let desc = img.desc.clone();
        let w = desc.width;
        let h = desc.height;

        tracing::info!(
            "[pixors] image loaded: {}×{} {} format={}",
            w, h, desc.bit_depth, desc.format
        );
        for meta in &desc.metadata {
            tracing::info!("[pixors] exif: {:20} = {}", meta.label(), meta.value_str());
        }

        let cache_dir = self.path.with_extension("pixors_cache");
        let vp_cache = ViewportCache::new();
        vp_cache.lock().unwrap().clear_all();
        vp_cache.lock().unwrap().signal_new_img(w, h);

        {
            let c = vp_cache.clone();
            install_viewport_cache_sink(Box::new(
                move |mip, tx, ty, px, py, tw, th, bytes| {
                    if let Ok(mut guard) = c.lock() {
                        guard.insert(
                            TileGridPos {
                                mip_level: mip,
                                tx,
                                ty,
                            },
                            CachedTile {
                                px,
                                py,
                                width: tw,
                                height: th,
                                bytes: bytes.to_vec(),
                            },
                        );
                    }
                },
            ));
        }

        let stream = Arc::new(Mutex::new(Some(
            img.open_page(0).map_err(|e| e.to_string())?,
        )));

        let pipe = PathBuilder::new()
            .src(ImageStreamSource {
                stream,
                image_height: desc.height,
            })
            .data_xform(ScanLineToTile {
                tile_size: TILE_SIZE,
                image_width: w,
                image_height: h,
            })
            .op(ColorConvert {
                target_format: PixelFormat::RgbaF16,
                target_color_space: ColorSpace::ACES_CG,
                target_alpha: AlphaPolicy::Straight,
            })
            .op(MipDownsample {
                image_width: w,
                image_height: h,
                tile_size: TILE_SIZE,
            })
            .op(ColorConvert {
                target_format: PixelFormat::Rgba8,
                target_color_space: ColorSpace::SRGB,
                target_alpha: AlphaPolicy::Straight,
            });

        let [pipe_cache, pipe_vp] = pipe.split();

        pipe_cache.sink(CacheWriter {
            cache_dir: cache_dir.clone(),
        });

        let graph = pipe_vp.sink(ViewportCacheSink).compile();

        let tab_id = state.alloc_tab_id();
        let title = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();

        let mut vs = ViewportState::default();
        vs.camera.img_w = w as f32;
        vs.camera.img_h = h as f32;

        state.push_tab(Tab {
            id: tab_id,
            title,
            source: TabSource::File {
                path: self.path.clone(),
            },
            desc,
            cache_dir,
            viewport_cache: vp_cache,
            viewport_state: Rc::new(RefCell::new(vs)),
            mip_fetch_signal: Arc::new(Mutex::new(Vec::new())),
            tile_generation: 0,
            layers: vec![],
            active_layer: None,
            chain: Default::default(),
            history: Default::default(),
            view: TabView {
                zoom: 1.0,
                pan: (0.0, 0.0),
                active_mip: 0,
                loading: true,
                progress: 0.0,
            },
        });

        *self.pending_tab_id.lock().unwrap() = Some(tab_id);

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            snapshot: None,
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        if let Some(tab_id) = *self.pending_tab_id.lock().unwrap()
            && let Some(tab) = state.tab_mut(tab_id) {
                tab.view.loading = false;
            }

        if let PipelineStatus::Error(e) = status {
            tracing::error!("OpenFile failed: {e}");
        }
    }

    fn undo(&self, state: &mut EditorState) {
        if let Some(id) = *self.pending_tab_id.lock().unwrap() {
            state.close(id);
        }
    }

    fn record_in_history(&self) -> bool {
        false
    }
}
