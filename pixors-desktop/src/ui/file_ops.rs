use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use pixors_executor::model::image::ImageFile;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::data_transform::{NeighborhoodAgg, ScanLineAccumulator, DataTransformNode};
use pixors_executor::operation::blur::Blur;
use pixors_executor::operation::OperationNode;
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::tile_sink::{install_tile_sink, TileSink};
use pixors_executor::sink::SinkNode;
use pixors_executor::source::SourceNode;
use pixors_executor::stage::StageNode;

use crate::viewport::program::{PendingTile, PendingTileWrites};

const TILE_SIZE: u32 = 256;
const BLUR_RADIUS: u32 = 32;

/// Install the tile-sink callback once. Subsequent calls are no-ops (OnceLock).
fn ensure_tile_sink_installed(pending: &Arc<PendingTileWrites>) {
    let p = pending.clone();
    install_tile_sink(Box::new(move |px, py, tw, th, bytes| {
        p.push_tile(PendingTile {
            px,
            py,
            tile_w: tw,
            tile_h: th,
            bytes: bytes.to_vec(),
        });
    }));
}

pub fn open_and_run(pending: &Arc<PendingTileWrites>) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let image = ImageFile::open(&path).map_err(|e| e.to_string())?;
    let w = image.width;
    let h = image.height;

    // Signal viewport realloc before the pipeline starts emitting tiles.
    pending.signal_realloc(w, h);
    *pending.new_img.lock().unwrap() = Some((w, h));

    ensure_tile_sink_installed(pending);

    // Build graph:
    //   ImageFileSource(ScanLine)
    //   → ScanLineAccumulator(Tile)
    //   → NeighborhoodAgg(Neighborhood)
    //   → Blur(Tile)
    //   → NeighborhoodAgg(Neighborhood)
    //   → Blur(Tile)
    //   → TileSink
    let mut graph = ExecGraph::new();
    let src   = graph.add_stage(StageNode::Source(SourceNode::ImageFile(image.source(0))));
    let acc   = graph.add_stage(StageNode::DataTransform(DataTransformNode::ScanLineAccumulator(
        ScanLineAccumulator { tile_size: TILE_SIZE },
    )));
    let nbhd1 = graph.add_stage(StageNode::DataTransform(DataTransformNode::NeighborhoodAgg(
        NeighborhoodAgg { radius: BLUR_RADIUS },
    )));
    let blur1 = graph.add_stage(StageNode::Operation(OperationNode::Blur(
        Blur { radius: BLUR_RADIUS },
    )));
    let nbhd2 = graph.add_stage(StageNode::DataTransform(DataTransformNode::NeighborhoodAgg(
        NeighborhoodAgg { radius: BLUR_RADIUS },
    )));
    let blur2 = graph.add_stage(StageNode::Operation(OperationNode::Blur(
        Blur { radius: BLUR_RADIUS },
    )));
    let sink  = graph.add_stage(StageNode::Sink(SinkNode::TileSink(TileSink)));

    graph.add_edge(src,   acc,   EdgePorts::default());
    graph.add_edge(acc,   nbhd1, EdgePorts::default());
    graph.add_edge(nbhd1, blur1, EdgePorts::default());
    graph.add_edge(blur1, nbhd2, EdgePorts::default());
    graph.add_edge(nbhd2, blur2, EdgePorts::default());
    graph.add_edge(blur2, sink,  EdgePorts::default());
    graph.outputs.push((sink, 0));

    let pipeline = Pipeline::compile(&graph).map_err(|e| e.to_string())?;
    thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] pipeline error: {e}");
        }
    });

    Ok((w, h, path.clone()))
}

pub fn probe_dimensions(path: &Path) -> Result<(u32, u32), String> {
    ImageFile::open(path).map(|i| (i.width, i.height)).map_err(|e| e.to_string())
}
