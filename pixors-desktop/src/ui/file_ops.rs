use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use pixors_executor::model::image::ImageFile;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::data_transform::{
    NeighborhoodAgg, ScanLineAccumulator, DataTransformNode,
};
use pixors_executor::operation::blur::Blur;
use pixors_executor::operation::mip_filter::MipFilter;
use pixors_executor::operation::mip_downsample::MipDownsample;
use pixors_executor::operation::OperationNode;
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::cache_writer::CacheWriter;
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
    install_tile_sink(Box::new(move |_mip, px, py, tw, th, bytes| {
        p.push_tile(PendingTile {
            px,
            py,
            tile_w: tw,
            tile_h: th,
            bytes: bytes.to_vec(),
        });
    }));
}

pub fn open_and_run(
    pending: &Arc<PendingTileWrites>,
    cache_dir: Option<PathBuf>,
) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let image = ImageFile::open(&path).map_err(|e| e.to_string())?;
    let w = image.width;
    let h = image.height;

    let cache_dir = cache_dir.unwrap_or_else(|| {
        path.with_extension("pixors_cache")
    });

    // Signal viewport realloc before the pipeline starts emitting tiles.
    pending.signal_realloc(w, h);
    *pending.new_img.lock().unwrap() = Some((w, h));

    ensure_tile_sink_installed(pending);

    // Build graph:
    //   ImageFileSource(ScanLine)
    //   → ScanLineAccumulator(Tile)
    //   → NeighborhoodAgg(Neighborhood)
    //   → Blur(Tile)
    //   → MipDownsample(Tile) — generates all MIP levels internally
    //   → CacheWriter — write all MIP tiles to disk
    //   → MipFilter(Tile, mip=0) → TileSink
    let mut graph = ExecGraph::new();
    let src    = graph.add_stage(StageNode::Source(SourceNode::ImageFile(image.source(0))));
    let acc    = graph.add_stage(StageNode::DataTransform(DataTransformNode::ScanLineAccumulator(
        ScanLineAccumulator { tile_size: TILE_SIZE },
    )));
    let nbhd   = graph.add_stage(StageNode::DataTransform(DataTransformNode::NeighborhoodAgg(
        NeighborhoodAgg { radius: BLUR_RADIUS },
    )));
    let blur   = graph.add_stage(StageNode::Operation(OperationNode::Blur(
        Blur { radius: BLUR_RADIUS },
    )));
    let mip    = graph.add_stage(StageNode::Operation(OperationNode::MipDownsample(
        MipDownsample { image_width: w, image_height: h, tile_size: TILE_SIZE },
    )));
    let cache  = graph.add_stage(StageNode::Sink(SinkNode::CacheWriter(
        CacheWriter { cache_dir },
    )));
    let filter = graph.add_stage(StageNode::Operation(OperationNode::MipFilter(
        MipFilter { mip_level: 0 },
    )));
    let sink   = graph.add_stage(StageNode::Sink(SinkNode::TileSink(TileSink)));

    graph.add_edge(src,    acc,    EdgePorts::default());
    graph.add_edge(acc,    nbhd,   EdgePorts::default());
    graph.add_edge(nbhd,   blur,   EdgePorts::default());
    graph.add_edge(blur,   mip,    EdgePorts::default());
    graph.add_edge(mip,    cache,  EdgePorts::default());
    graph.add_edge(mip,    filter, EdgePorts::default());
    graph.add_edge(filter, sink,   EdgePorts::default());
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
