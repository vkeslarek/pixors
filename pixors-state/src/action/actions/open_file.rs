use std::fmt;
use std::sync::{Arc, Mutex};

use pixors_ops::processor::color::ColorConvert;
use pixors_engine::common::pixel::AlphaPolicy;
use pixors_engine::stage::Stage;
use pixors_image::image::open_image;
use pixors_image::sink::cache_writer::CacheWriter;
use pixors_image::source::image_stream::ImageStreamSource;
use pixors_ops::processor::mip_downsample::MipDownsample;

use crate::PathBuilder;
use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::tab::{BlendMode, FilterState, Layer, LayerSource};
use crate::{EditorState, Tab, TabId, TabSource, TabView};

use crate::TILE_SIZE;

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
        let tab_id = state.alloc_tab_id();

        let stream = Arc::new(Mutex::new(Some(
            img.open_page(0).map_err(|e| e.to_string())?,
        )));

        let graph = PathBuilder::new()
            .src(Stage::Producer(Box::new(ImageStreamSource {
                stream,
                image_height: desc.height,
            })))
            .data_xform(Stage::Processor(Box::new(
                pixors_engine::data_transform::to_tile::ScanLineToTile::new(TILE_SIZE, w, h),
            )))
            .op(Stage::Processor(Box::new(ColorConvert {
                target_format: state.working_format,
                target_color_space: state.working_color_space,
                target_alpha: AlphaPolicy::Straight,
            })))
            .op(Stage::Processor(Box::new(MipDownsample::new(w, h, TILE_SIZE))))
            .sink(Stage::Consumer(Box::new(CacheWriter {
                cache_dir: cache_dir.clone(),
            })))
            .compile();

        let title = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();

        let num_pages = img.page_count();
        let mut layers: Vec<Layer> = Vec::with_capacity(num_pages);
        let mut active_layer = None;
        for page in 0..num_pages {
            let layer_id = state.alloc_layer_id();
            if page == 0 {
                active_layer = Some(layer_id);
            }
            let name = desc
                .pages
                .get(page)
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

        let tab = Tab {
            id: tab_id,
            title,
            source: TabSource::File {
                path: self.path.clone(),
            },
            desc,
            cache_dir,
            redraw_seq: 0,
            layers,
            active_layer,
            history: Default::default(),
            view: TabView {
                active_mip: 0,
                loading: true,
                progress: 0.0,
            },
            filter: FilterState::default(),
        };

        *self.pending_tab.lock().unwrap() = Some(tab);

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
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
