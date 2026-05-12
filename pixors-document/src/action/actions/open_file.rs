use std::fmt;
use std::sync::{Arc, Mutex};

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::graph::graph::{EdgePorts, ExecGraph};
use pixors_engine::stage::Stage;
use pixors_image::image::{BlendMode, open_image};
use pixors_image::source::image_stream::ImageStreamSource;
use pixors_ops::processor::color::ColorConvert;
use pixors_ops::processor::mip_downsample::MipDownsample;

use crate::action::{Action, PipelineMode, PipelineStatus, PreparedAction};
use crate::document::{BlendSpec, CanvasInfo, Document, LayerNode, PixelSource};
use crate::session::Transient;
use crate::{EditorState, Session, SessionId, ViewState};

use crate::TILE_SIZE;

pub struct OpenFile {
    pub path: std::path::PathBuf,
    pending_tab: Mutex<Option<Session>>,
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
    fn target_tab(&self) -> Option<SessionId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let img = open_image(&self.path).map_err(|e| e.to_string())?;
        let desc = img.desc.clone();
        let (w, h) = (desc.width, desc.height);

        tracing::info!(
            "[pixors] image loaded: {w}×{h} {} format={}",
            desc.bit_depth,
            desc.format
        );
        for meta in &desc.metadata {
            tracing::info!("[pixors] exif: {:20} = {}", meta.label(), meta.value_str());
        }

        let session_id = state.alloc_session_id();
        let cache_dir = std::env::temp_dir()
            .join("pixors")
            .join(format!("session_{:016x}", session_id.0));

        let mut transient = Transient::new(cache_dir.clone());

        let mut document = Document::new(CanvasInfo {
            width: w,
            height: h,
            working_color_space: ColorSpace::ACES_CG,
            working_format: PixelFormat::RgbaF16,
        });
        document.assets.primary_path = Some(self.path.clone());

        let num_pages = img.page_count();
        let mut layers = Vec::with_capacity(num_pages);
        for page in 0..num_pages {
            let id = document.alloc_node_id();
            let name = desc
                .pages
                .get(page)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Page {}", page + 1));
            layers.push(LayerNode {
                id,
                name,
                visible: page == 0,
                blend: BlendSpec {
                    mode: BlendMode::Normal,
                    opacity: 1.0,
                },
                source: PixelSource::PrimaryAsset { page },
                transforms: Vec::new(),
                mask: None,
            });
        }
        document.layers = layers;

        let mut graph = ExecGraph::new();
        for layer in &document.layers {
            let page = match &layer.source {
                PixelSource::PrimaryAsset { page } => *page,
                _ => continue,
            };
            let stream = Arc::new(Mutex::new(Some(
                img.open_page(page).map_err(|e| e.to_string())?,
            )));
            let src = graph.add_stage(Stage::Producer(Box::new(ImageStreamSource {
                stream,
                image_height: h,
            })));
            let to_tile = graph.add_stage(Stage::Processor(Box::new(
                pixors_engine::data_transform::to_tile::ScanLineToTile::new(TILE_SIZE, w, h),
            )));
            graph.add_edge(
                src,
                to_tile,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );
            let color = graph.add_stage(Stage::Processor(Box::new(ColorConvert {
                target_format: PixelFormat::RgbaF16,
                target_color_space: ColorSpace::ACES_CG,
                target_alpha: AlphaPolicy::Straight,
            })));
            graph.add_edge(
                to_tile,
                color,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );
            let mip = graph.add_stage(Stage::Processor(Box::new(MipDownsample::new(
                w, h, TILE_SIZE,
            ))));
            graph.add_edge(
                color,
                mip,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );
            let disk = transient.get_or_create_disk_cache(layer.id);
            let writer = graph.add_stage(Stage::Consumer(Box::new(
                pixors_engine::cache::cache_writer::CacheWriter::new(disk),
            )));
            graph.add_edge(
                mip,
                writer,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );
        }

        transient.view = ViewState {
            active_mip: 0,
            loading: true,
            progress: 0.0,
        };
        if let Some(first) = document.layers.first() {
            transient.active_node = Some(first.id);
        }

        let new_session = Session {
            id: session_id,
            document,
            history: Default::default(),
            transient,
            working_format: pixors_engine::common::pixel::PixelFormat::RgbaF16,
            working_color_space: pixors_engine::common::color::space::ColorSpace::ACES_CG,
            display_format: pixors_engine::common::pixel::PixelFormat::Rgba8,
            display_color_space: pixors_engine::common::color::space::ColorSpace::SRGB,
        };
        *self.pending_tab.lock().unwrap() = Some(new_session);

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph,
            routed_tab: Some(session_id),
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        match status {
            PipelineStatus::Done => {
                if let Some(s) = self.pending_tab.lock().unwrap().take() {
                    state.push(s);
                }
            }
            PipelineStatus::Error(e) => tracing::error!("OpenFile failed: {e}"),
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
