use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::data_transform::to_neighborhood::TileToNeighborhood;
use pixors_engine::graph::graph::{EdgePorts, ExecGraph, StageId};
use pixors_engine::stage::Stage;
use pixors_ops::processor::blur::Blur;
use pixors_ops::processor::color::ColorConvert;
use pixors_ops::processor::compose::Compose;
use pixors_ops::source::cache_reader::{CacheReader, TileRange};

use crate::document::transform::{InputScope, Operation, OutputMode, Transform};
use crate::document::{BlendSpec, Document, LayerNode, NodeId};
use pixors_image::image::BlendMode;

// ── Public surface ───────────────────────────────────────────────────────────

/// Runtime configuration supplied by the caller (controller / MCP).
/// Separates caller-specific settings from pure document logic.
pub struct CompileConfig {
    /// Root cache directory for this tab (layer tile caches live as subdirs).
    pub cache_dir: PathBuf,
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
    pub tile_size: u32,
    /// Mip-0 image dimensions — used to compute mip-adjusted sizes.
    pub img_w: u32,
    pub img_h: u32,
}

pub struct RenderRequest {
    pub viewport: TileRange,
    pub mip_level: u32,
    /// If set, stop compilation before this node (for "show before" previews).
    pub up_to: Option<NodeId>,
}

/// Compile a Document into an ExecGraph ready to run.
///
/// Pure function — no mutation of `doc`, no side effects.
/// `sink` is caller-provided (e.g. TileCacheSink from pixors-desktop).
pub fn compile(
    doc: &Document,
    req: &RenderRequest,
    config: &CompileConfig,
    sink: Stage,
) -> ExecGraph {
    let mut ctx = CompileCtx::new(doc, req, config);
    let color_out = compile_layer_stack(&doc.layers, &mut ctx);
    let sink_id = ctx.graph.add_stage(sink);
    ctx.graph.add_edge(
        color_out,
        sink_id,
        EdgePorts {
            from_port: 0,
            to_port: 0,
        },
    );
    ctx.finish()
}

/// Like [`compile`] but inserts a temporary `preview_op` transform on
/// `preview_layer_id` before compilation. The document is not mutated —
/// layers are cloned. Used for live filter previews (blur slider etc.).
pub fn compile_preview(
    doc: &Document,
    req: &RenderRequest,
    config: &CompileConfig,
    sink: Stage,
    preview_layer_id: NodeId,
    preview_op: &Operation,
) -> ExecGraph {
    let mut ctx = CompileCtx::new(doc, req, config);
    let mut layers = doc.layers.clone();
    if let Some(layer) = layers.iter_mut().find(|l| l.id == preview_layer_id) {
        layer.transforms.push(Transform {
            id: NodeId(0),
            op: preview_op.clone(),
            input: InputScope::Layer,
            output: OutputMode::Replace {
                blend: BlendSpec {
                    mode: BlendMode::Normal,
                    opacity: 1.0,
                },
            },
            enabled: true,
        });
    }
    let color_out = compile_layer_stack(&layers, &mut ctx);
    let sink_id = ctx.graph.add_stage(sink);
    ctx.graph.add_edge(
        color_out,
        sink_id,
        EdgePorts {
            from_port: 0,
            to_port: 0,
        },
    );
    ctx.finish()
}

// ── Compiler context ─────────────────────────────────────────────────────────

struct CompileCtx<'a> {
    #[allow(dead_code)]
    doc: &'a Document,
    req: &'a RenderRequest,
    config: &'a CompileConfig,
    graph: ExecGraph,
    compiled_nodes: HashMap<NodeId, StageId>,
    compilation_stack: HashSet<NodeId>,
}

impl<'a> CompileCtx<'a> {
    fn new(doc: &'a Document, req: &'a RenderRequest, config: &'a CompileConfig) -> Self {
        Self {
            doc,
            req,
            config,
            graph: ExecGraph::new(),
            compiled_nodes: HashMap::new(),
            compilation_stack: HashSet::new(),
        }
    }

    fn finish(self) -> ExecGraph {
        self.graph
    }

    fn layer_cache_dir(&self, layer_id: NodeId) -> PathBuf {
        crate::document::cache::layer_cache_dir(&self.config.cache_dir, layer_id)
    }
}

// ── Layer stack ───────────────────────────────────────────────────────────────

fn compile_layer_stack(layers: &[LayerNode], ctx: &mut CompileCtx) -> StageId {
    let visible: Vec<&LayerNode> = layers
        .iter()
        .filter(|l| l.visible && ctx.layer_cache_dir(l.id).exists())
        .collect();

    let n = visible.len();
    assert!(
        n > 0,
        "compile() called with no visible layers — guard in caller"
    );

    let compose = ctx.graph.add_stage(Stage::Processor(Box::new(Compose::new(
        n as u16,
        visible.iter().map(|l| l.blend.mode).collect(),
        visible.iter().map(|l| l.blend.opacity).collect(),
    ))));

    for (i, layer) in visible.iter().enumerate() {
        let layer_out = compile_layer(layer, ctx);
        ctx.graph.add_edge(
            layer_out,
            compose,
            EdgePorts {
                from_port: 0,
                to_port: i as u16,
            },
        );
    }

    let color_out = ctx.graph.add_stage(Stage::Processor(Box::new(ColorConvert {
        target_format: ctx.config.display_format,
        target_color_space: ctx.config.display_color_space,
        target_alpha: AlphaPolicy::Straight,
    })));
    ctx.graph.add_edge(
        compose,
        color_out,
        EdgePorts {
            from_port: 0,
            to_port: 0,
        },
    );

    color_out
}

// ── Single layer ─────────────────────────────────────────────────────────────

fn compile_layer(layer: &LayerNode, ctx: &mut CompileCtx) -> StageId {
    let mut current = compile_layer_content(layer, ctx);

    for t in &layer.transforms {
        current = compile_transform(t, current, None, ctx);
    }

    current
}

fn compile_layer_content(layer: &LayerNode, ctx: &mut CompileCtx) -> StageId {
    use crate::document::PixelSource;
    match &layer.source {
        PixelSource::PrimaryAsset { .. } => {
            let cache_dir = ctx.layer_cache_dir(layer.id);
            ctx.graph.add_stage(Stage::Producer(Box::new(CacheReader {
                cache_dir,
                mip_level: ctx.req.mip_level,
                tile_size: ctx.config.tile_size,
                image_width: ctx.config.img_w,
                image_height: ctx.config.img_h,
                tile_range: Some(ctx.req.viewport.clone()),
                pixel_format: ctx.config.working_format,
                color_space: ctx.config.working_color_space,
            })))
        }
        PixelSource::SolidColor { .. } => {
            todo!("SolidColor layer content not yet implemented in render compiler")
        }
    }
}

// ── Transform ─────────────────────────────────────────────────────────────────

fn compile_transform(
    t: &Transform,
    layer_input: StageId,
    below_input: Option<StageId>,
    ctx: &mut CompileCtx,
) -> StageId {
    if !t.enabled {
        return layer_input;
    }

    let input = match &t.input {
        InputScope::Layer => layer_input,
        InputScope::Below => below_input.unwrap_or_else(|| {
            tracing::warn!(
                "Transform {:?}: InputScope::Below but no below available",
                t.id
            );
            layer_input
        }),
        InputScope::Reference(other) => compile_reference(*other, ctx),
    };

    let op_stage = compile_operation(&t.op, input, ctx);

    match &t.output {
        OutputMode::Replace { .. } => op_stage,
        OutputMode::Composite { .. } => {
            todo!("OutputMode::Composite not yet implemented")
        }
    }
}

fn compile_operation(op: &Operation, input: StageId, ctx: &mut CompileCtx) -> StageId {
    match op {
        Operation::Blur { radius } => {
            let ttn = ctx
                .graph
                .add_stage(Stage::Processor(Box::new(TileToNeighborhood::new(
                    *radius as u32,
                ))));
            ctx.graph.add_edge(
                input,
                ttn,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );

            let blur = ctx.graph.add_stage(Stage::Processor(Box::new(Blur {
                radius: *radius as u32,
            })));
            ctx.graph.add_edge(
                ttn,
                blur,
                EdgePorts {
                    from_port: 0,
                    to_port: 0,
                },
            );

            blur
        }
        Operation::Exposure { .. } => {
            todo!("Operation::Exposure not yet implemented")
        }
    }
}

fn compile_reference(target: NodeId, ctx: &mut CompileCtx) -> StageId {
    if let Some(&s) = ctx.compiled_nodes.get(&target) {
        return s;
    }
    if !ctx.compilation_stack.insert(target) {
        tracing::error!("Circular reference involving {:?}", target);
        return todo_stage(ctx);
    }
    let result = todo_stage(ctx); // TODO: resolve target in doc, compile isolated
    ctx.compilation_stack.remove(&target);
    ctx.compiled_nodes.insert(target, result);
    result
}

fn todo_stage(_ctx: &mut CompileCtx) -> StageId {
    todo!("render compiler path not yet implemented")
}
