use std::fmt;
use std::sync::{Arc, Mutex};

use pixors_image::common::image::open_image;
use pixors_engine::common::pixel::AlphaPolicy;
use pixors_engine::data::tile::TileGridPos;
use pixors_engine::data_transform::to_tile::ScanLineToTile;
use pixors_color::operation::color::ColorConvert;
use pixors_ops::operation::mip_downsample::MipDownsample;
use pixors_image::sink::cache_writer::CacheWriter;
use crate::viewport_cache_sink::{ViewportCacheSink, register_tab_cache};
use pixors_image::source::image_stream::ImageStreamSource;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::path_builder::PathBuilder;
use crate::state::tab::{BlendMode, FilterState, Layer, LayerSource};
use crate::state::{EditorState, Tab, TabId, TabSource, TabView};
use crate::viewport::state::ViewportState;
use crate::viewport::tile_cache::{CachedTile, ViewportCache};

const TILE_SIZE: u32 = 256;

pub struct OpenFile {
    pub path: std::path::PathBuf,
    pending_tab: Mutex<Option<Tab>>,
}

impl fmt::Debug for OpenFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenFile")
            .field("path", &self.path)
            .finish()
    }
}

impl OpenFile {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            pending_tab: Mutex::new(None),
        }
    }
}

impl Action for OpenFile {
    fn target_tab(&self) -> Option<TabId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let img = open_image(&self.path).map_err(|e| e.to_string())?;
        let desc = img.desc.clone();
        let w = desc.width;
        let h = desc.height;

        tracing::info!(
            "[pixors] image loaded: {}×{} {} format={}",
            w,
            h,
            desc.bit_depth,
            desc.format
        );
        for meta in &desc.metadata {
            tracing::info!("[pixors] exif: {:20} = {}", meta.label(), meta.value_str());
        }

        let cache_dir = self.path.with_extension("pixors_cache");
        let vp_cache = ViewportCache::new();
        vp_cache.lock().unwrap().clear_all();
        vp_cache.lock().unwrap().signal_new_img(w, h);

        let tab_id = state.alloc_tab_id();

        {
            let c = vp_cache.clone();
            register_tab_cache(
                tab_id.0,
                Box::new(move |generation, mip, tx, ty, px, py, tw, th, bytes| {
                    if let Ok(mut guard) = c.lock() {
                        guard.insert(
                            generation,
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
                                bytes: Arc::new(bytes.to_vec()),
                                generation,
                            },
                        );
                    }
                }),
            );
        }

        let stream = Arc::new(Mutex::new(Some(
            img.open_page(0).map_err(|e| e.to_string())?,
        )));

        let pipe = PathBuilder::new()
            .src(Arc::new(ImageStreamSource {
                stream,
                image_height: desc.height,
            }))
            .data_xform(Arc::new(ScanLineToTile {
                tile_size: TILE_SIZE,
                image_width: w,
                image_height: h,
            }))
            .op(Arc::new(ColorConvert {
                target_format: state.working_format,
                target_color_space: state.working_color_space,
                target_alpha: AlphaPolicy::Straight,
            }))
            .op(Arc::new(MipDownsample {
                image_width: w,
                image_height: h,
                tile_size: TILE_SIZE,
            }));

        let [pipe_cache, pipe_vp] = pipe.split();

        pipe_cache.sink(Arc::new(CacheWriter {
            cache_dir: cache_dir.clone(),
        }));

        let graph = pipe_vp
            .op(Arc::new(ColorConvert {
                target_format: state.display_format,
                target_color_space: state.display_color_space,
                target_alpha: AlphaPolicy::Straight,
            }))
            .sink(Arc::new(ViewportCacheSink::new(tab_id.0, 0)))
            .compile();
        let title = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();

        let mut vs = ViewportState::default();
        vs.camera.img_w = w as f32;
        vs.camera.img_h = h as f32;

        let num_pages = img.page_count();
        let mut layers: Vec<Layer> = Vec::with_capacity(num_pages);
        let mut active_layer = None;
        for page in 0..num_pages {
            let layer_id = state.alloc_layer_id();
            if page == 0 {
                active_layer = Some(layer_id);
            }
            let name = desc.pages.get(page)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Page {}", page + 1));
            layers.push(Layer {
                id: layer_id,
                name,
                visible: page == 0,
                opacity: 1.0,
                blend: BlendMode::Normal,
                source: LayerSource::FilePage { page },
            });
        }
        let active_layer = active_layer;

        let tab = Tab {
            id: tab_id,
            title,
            source: TabSource::File {
                path: self.path.clone(),
            },
            desc,
            cache_dir,
            viewport_cache: vp_cache,
            viewport_state: Arc::new(std::sync::RwLock::new(vs)),
            mip_fetch_signal: Arc::new(Mutex::new(Vec::new())),
            tile_generation: 0,
            layers,
            active_layer,
            chain: Default::default(),
            history: Default::default(),
            view: TabView {
                zoom: 1.0,
                pan: (0.0, 0.0),
                active_mip: 0,
                loading: true,
                progress: 0.0,
                preview_gen: 0,
            },
            filter: FilterState::default(),
        };

        *self.pending_tab.lock().unwrap() = Some(tab);

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            snapshot: None,
            routed_tab: Some(tab_id),
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        match status {
            PipelineStatus::Done => {
                if let Some(tab) = self.pending_tab.lock().unwrap().take() {
                    state.push_tab(tab);
                }
            }
            PipelineStatus::Error(e) => {
                tracing::error!("OpenFile failed: {e}");
            }
            PipelineStatus::Cancelled => {}
        }
    }

    fn undo(&self, state: &mut EditorState) {
        if let Some(tab) = self.pending_tab.lock().unwrap().take() {
            state.close(tab.id);
        }
    }

    fn record_in_history(&self) -> bool {
        false
    }
}
