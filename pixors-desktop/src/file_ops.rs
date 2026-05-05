use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use pixors_executor::data::tile::TileGridPos;
use pixors_executor::model::image::ImageFile;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph};
use pixors_executor::data_transform::to_tile::ScanLineToTile;
use pixors_executor::data_transform::DataTransformNode;
use pixors_executor::model::image::BlendMode;
use pixors_executor::operation::compose::Compose;
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
use pixors_executor::source::cache_reader::{CacheReader, TileRange};
use pixors_executor::source::SourceNode;
use pixors_executor::stage::StageNode;

use crate::viewport::tile_cache::{CachedTile, ViewportCache};

const TILE_SIZE: u32 = 256;

fn ensure_tile_sink_installed() {
    install_tile_sink(Box::new(|_, _, _, _, _, _| {}));
}

pub fn open_and_run(
    vp_cache: Option<Arc<Mutex<ViewportCache>>>,
) -> Result<(u32, u32, PathBuf), String> {
    let path_a = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let path_b = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .set_title("Pick second image to overlay")
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let img_a = ImageFile::open(&path_a).map_err(|e| e.to_string())?;
    let img_b = ImageFile::open(&path_b).map_err(|e| e.to_string())?;
    let w = img_a.width.max(img_b.width);
    let h = img_a.height.max(img_b.height);

    let cache_dir = path_a.with_extension("pixors_cache");

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

    let src_a = graph.add_stage(StageNode::Source(SourceNode::ImageFile(img_a.source(0))));
    let acc_a = graph.add_stage(StageNode::DataTransform(DataTransformNode::ScanLineToTile(
        ScanLineToTile { tile_size: TILE_SIZE },
    )));
    let src_b = graph.add_stage(StageNode::Source(SourceNode::ImageFile(img_b.source(0))));
    let acc_b = graph.add_stage(StageNode::DataTransform(DataTransformNode::ScanLineToTile(
        ScanLineToTile { tile_size: TILE_SIZE },
    )));

    let compose = graph.add_stage(StageNode::Operation(OperationNode::Compose(
        Compose { layer_count: 2, blend_modes: vec![BlendMode::Normal; 2] },
    )));

    let mip = graph.add_stage(StageNode::Operation(OperationNode::MipDownsample(
        MipDownsample { image_width: w, image_height: h, tile_size: TILE_SIZE },
    )));
    let cache = graph.add_stage(StageNode::Sink(SinkNode::CacheWriter(
        CacheWriter { cache_dir },
    )));
    let vp_sink = graph.add_stage(StageNode::Sink(SinkNode::ViewportCacheSink(
        ViewportCacheSink,
    )));
    let filter = graph.add_stage(StageNode::Operation(OperationNode::MipFilter(
        MipFilter { mip_level: 0 },
    )));
    let sink = graph.add_stage(StageNode::Sink(SinkNode::TileSink(TileSink)));

    graph.add_edge(src_a, acc_a, EdgePorts::default());
    graph.add_edge(src_b, acc_b, EdgePorts::default());
    // port 0 = first image (top), port 1 = second image (bottom)
    graph.add_edge(acc_a, compose, EdgePorts::default());
    graph.add_edge(acc_b, compose, EdgePorts { from_port: 0, to_port: 1 });
    graph.add_edge(compose, mip, EdgePorts::default());
    graph.add_edge(mip, cache, EdgePorts::default());
    graph.add_edge(mip, vp_sink, EdgePorts::default());
    graph.add_edge(mip, filter, EdgePorts::default());
    graph.add_edge(filter, sink, EdgePorts::default());
    graph.outputs.push((sink, 0));

    let pipeline = Pipeline::compile(&graph).map_err(|e| e.to_string())?;
    thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] pipeline error: {e}");
        }
    });

    Ok((w, h, path_a))
}

pub fn fetch_mip(
    cache_dir: &Path,
    mip: u32,
    range: TileRange,
    img_w: u32,
    img_h: u32,
    vp_cache: Arc<Mutex<ViewportCache>>,
) {
    let _ = vp_cache;

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
    let vp = graph.add_stage(StageNode::Sink(SinkNode::ViewportCacheSink(ViewportCacheSink)));
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
