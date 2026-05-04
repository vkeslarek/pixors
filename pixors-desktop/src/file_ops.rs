use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use pixors_executor::data::TileGridPos;
use pixors_executor::model::image::ImageFile;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::data_transform::{ScanLineAccumulator, DataTransformNode};
use pixors_executor::operation::mip_filter::MipFilter;
use pixors_executor::operation::mip_downsample::MipDownsample;
use pixors_executor::operation::OperationNode;
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::cache_writer::CacheWriter;
use pixors_executor::sink::tile_sink::{install_tile_sink, TileSink};
use pixors_executor::sink::viewport_cache_sink::{
    install_viewport_cache_sink, ViewportCacheSink,
};
use pixors_executor::sink::SinkNode;
use pixors_executor::source::{CacheReader, SourceNode, TileRange};
use pixors_executor::stage::StageNode;

use crate::viewport::tile_cache::{CachedTile, ViewportCache};

const TILE_SIZE: u32 = 256;

/// Install a no-op tile-sink callback once. Keeps the pipeline graph valid
/// (TileSink acts as the output node that drives execution).
fn ensure_tile_sink_installed() {
    install_tile_sink(Box::new(|_, _, _, _, _, _| {}));
}

pub fn open_and_run(
    vp_cache: Option<Arc<Mutex<ViewportCache>>>,
) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let image = ImageFile::open(&path).map_err(|e| e.to_string())?;
    let w = image.width;
    let h = image.height;

    let cache_dir = path.with_extension("pixors_cache");

    if let Some(ref cache) = vp_cache {
        if let Ok(mut guard) = cache.lock() {
            guard.clear_all();
            guard.signal_new_img(w, h);
        }
    }

    ensure_tile_sink_installed();

    if let Some(ref cache) = vp_cache {
        let c = cache.clone();
        install_viewport_cache_sink(Box::new(
            move |mip: u32, tx: u32, ty: u32, px: u32, py: u32, tw: u32, th: u32, bytes: &[u8]| {
                if let Ok(mut guard) = c.lock() {
                    guard.insert(
                        TileGridPos { mip_level: mip, tx, ty },
                        CachedTile { px, py, width: tw, height: th, bytes: bytes.to_vec() },
                    );
                }
            },
        ));
    }

    let mut graph = ExecGraph::new();
    let src    = graph.add_stage(StageNode::Source(SourceNode::ImageFile(image.source(0))));
    let acc    = graph.add_stage(StageNode::DataTransform(DataTransformNode::ScanLineAccumulator(
        ScanLineAccumulator { tile_size: TILE_SIZE },
    )));
    let mip    = graph.add_stage(StageNode::Operation(OperationNode::MipDownsample(
        MipDownsample { image_width: w, image_height: h, tile_size: TILE_SIZE },
    )));
    let cache  = graph.add_stage(StageNode::Sink(SinkNode::CacheWriter(
        CacheWriter { cache_dir },
    )));
    let vp_sink = graph.add_stage(StageNode::Sink(SinkNode::ViewportCacheSink(
        ViewportCacheSink,
    )));
    let filter = graph.add_stage(StageNode::Operation(OperationNode::MipFilter(
        MipFilter { mip_level: 0 },
    )));
    let sink   = graph.add_stage(StageNode::Sink(SinkNode::TileSink(TileSink)));

    graph.add_edge(src,    acc,     EdgePorts::default());
    graph.add_edge(acc,    mip,     EdgePorts::default());
    graph.add_edge(mip,    cache,   EdgePorts::default());
    graph.add_edge(mip,    vp_sink, EdgePorts::default());
    graph.add_edge(mip,    filter,  EdgePorts::default());
    graph.add_edge(filter, sink,    EdgePorts::default());
    graph.outputs.push((sink, 0));

    let pipeline = Pipeline::compile(&graph).map_err(|e| e.to_string())?;
    thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] pipeline error: {e}");
        }
    });

    Ok((w, h, path))
}

/// Fetch only the visible tiles for `mip` from disk → ViewportCacheSink → ViewportCache.
/// Reuses the same CacheCommitFn already installed by `open_and_run`.
pub fn fetch_mip(
    cache_dir: &Path,
    mip: u32,
    range: TileRange,
    img_w: u32,
    img_h: u32,
    vp_cache: Arc<Mutex<ViewportCache>>,
) {
    // Re-ensure the ViewportCacheSink callback points to the current cache.
    // OnceLock: first call wins, so this is only effective on first image open.
    // Re-opens clear the cache but reuse the same Arc, so the callback remains valid.
    let _ = vp_cache; // ownership kept alive via callback installed in open_and_run

    let reader = CacheReader {
        cache_dir: cache_dir.to_owned(),
        mip_level: mip,
        tile_size: TILE_SIZE,
        image_width: img_w,
        image_height: img_h,
        tile_range: Some(range),
    };

    let mut graph = ExecGraph::new();
    let src = graph.add_stage(StageNode::Source(SourceNode::CacheReader(reader)));
    let vp  = graph.add_stage(StageNode::Sink(SinkNode::ViewportCacheSink(ViewportCacheSink)));
    graph.add_edge(src, vp, EdgePorts::default());
    graph.outputs.push((vp, 0));

    let Ok(pipeline) = Pipeline::compile(&graph) else {
        tracing::warn!("[pixors] fetch_mip {mip}: compile failed (cache not on disk yet?)");
        return;
    };
    thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] fetch_mip {mip} error: {e}");
        }
    });
}

pub fn probe_dimensions(path: &Path) -> Result<(u32, u32), String> {
    ImageFile::open(path).map(|i| (i.width, i.height)).map_err(|e| e.to_string())
}
