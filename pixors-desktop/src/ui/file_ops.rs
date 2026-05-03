use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixors_executor::model::image::ImageFile;
use pixors_executor::sink::viewport::ViewportSink;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::operation::blur::Blur;
use pixors_executor::operation::OperationNode;
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::SinkNode;
use pixors_executor::source::SourceNode;
use pixors_executor::stage::StageNode;

/// Set by open_and_run, consumed by the render thread to launch the pipeline
/// after the viewport texture is created.
pub struct PendingPipeline {
    pub graph: ExecGraph,
}

/// Open a file dialog, get metadata, signal the render thread.
pub fn open_and_run(pending: &Arc<crate::viewport::program::PendingTileWrites>) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let image = ImageFile::open(&path).map_err(|e| e.to_string())?;
    let w = image.width;
    let h = image.height;

    *pending.realloc.lock().unwrap() = Some((w, h));
    *pending.new_img.lock().unwrap() = Some((w, h));

    let mut graph = ExecGraph::new();
    let src = graph.add_stage(StageNode::Source(SourceNode::ImageFile(image.source(0))));
    let mut prev = src;
    for _ in 0..2 {
        let b = graph.add_stage(StageNode::Operation(OperationNode::Blur(Blur { radius: 8 })));
        graph.add_edge(prev, b, EdgePorts::default());
        prev = b;
    }
    let sink = graph.add_stage(StageNode::Sink(SinkNode::Viewport(ViewportSink { width: w, height: h })));
    graph.add_edge(prev, sink, EdgePorts::default());
    graph.outputs.push((sink, 0));

    *pending.pipeline.lock().unwrap() = Some(PendingPipeline { graph });

    Ok((w, h, path.clone()))
}

pub fn probe_dimensions(path: &Path) -> Result<(u32, u32), String> {
    ImageFile::open(path).map(|i| (i.width, i.height)).map_err(|e| e.to_string())
}
