use std::path::{Path, PathBuf};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread;
use pixors_executor::data::tile::TileGridPos;
use pixors_executor::common::image::Image;
use pixors_executor::data_transform::to_tile::ScanLineToTile;
use pixors_executor::common::color::space::ColorSpace;
use pixors_executor::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_executor::operation::color::ColorConvert;
use pixors_executor::operation::mip_filter::MipFilter;
use pixors_executor::operation::mip_downsample::MipDownsample;
use pixors_executor::runtime::event::PipelineEvent;
use pixors_executor::runtime::pipeline::Pipeline;
use pixors_executor::sink::cache_writer::CacheWriter;
use pixors_executor::sink::tile_sink::{install_tile_sink, TileSink};
use pixors_executor::sink::viewport_cache_sink::{
    install_viewport_cache_sink, ViewportCacheSink,
};
use pixors_executor::sink::SinkNode;
use pixors_executor::source::cache_reader::{CacheReader, TileRange};
use pixors_executor::source::image_stream::ImageStreamSource;
use pixors_executor::common::image::codec::EncoderConfig;

use crate::path_builder::PathBuilder;
use crate::viewport::tile_cache::{CachedTile, ViewportCache};

const TILE_SIZE: u32 = 256;

fn ensure_tile_sink_installed() {
    install_tile_sink(Box::new(|_, _, _, _, _, _| {}));
}

pub fn open_and_run(
    vp_cache: Option<Arc<Mutex<ViewportCache>>>,
) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let img = Image::open(&path).map_err(|e| e.to_string())?;
    let w = img.desc.width;
    let h = img.desc.height;

    tracing::info!("[pixors] image loaded: {}×{} {} format={}",
        w, h, img.desc.bit_depth, img.desc.format);
    tracing::info!("[pixors] color_space={:?} dpi={:?} pages={}",
        img.desc.color_space, img.desc.dpi, img.page_count());
    for meta in &img.desc.metadata {
        tracing::info!("[pixors] exif: {:20} = {}", meta.label(), meta.value_str());
    }

    let cache_dir = path.with_extension("pixors_cache");

    if let Some(ref cache) = vp_cache
        && let Ok(mut guard) = cache.lock() {
            guard.clear_all();
            guard.signal_new_img(w, h);
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

    let stream = Arc::new(Mutex::new(Some(
        img.open_page(0).map_err(|e| e.to_string())?
    )));

    let pipe = PathBuilder::new()
        .src(ImageStreamSource { stream, image_height: img.desc.height })
        .data_xform(ScanLineToTile { tile_size: TILE_SIZE, image_width: w, image_height: h })
        .op(ColorConvert { target_format: PixelFormat::RgbaF16, target_color_space: ColorSpace::ACES_CG, target_alpha: AlphaPolicy::Straight })
        .op(MipDownsample { image_width: w, image_height: h, tile_size: TILE_SIZE })
        .op(ColorConvert { target_format: PixelFormat::Rgba8, target_color_space: ColorSpace::SRGB, target_alpha: AlphaPolicy::Straight });

    let [pipe_cache, pipe_vp, pipe_graph] = pipe.split();

    pipe_cache
        .sink(CacheWriter { cache_dir: cache_dir.clone() });

    pipe_vp
        .sink(ViewportCacheSink);

    let graph = pipe_graph
        .op(MipFilter { mip_level: 0 })
        .sink(TileSink)
        .mark_output(0)
        .compile();

    let (event_tx, event_rx) = sync_channel::<PipelineEvent>(64);
    let pipeline = Pipeline::compile(&graph, Some(event_tx.clone())).map_err(|e| e.to_string())?;
    thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] pipeline error: {e}");
        }
        let _ = event_tx.send(PipelineEvent::Done);
    });
    let broadcast_tx = crate::app::pipeline_event_tx();
    thread::spawn(move || {
        while let Ok(event) = event_rx.recv() {
            let _ = broadcast_tx.send(event);
        }
    });

    Ok((w, h, path))
}

pub fn fetch_mip(
    cache_dir: &Path,
    mip: u32,
    range: TileRange,
    img_w: u32,
    img_h: u32,
    _vp_cache: Arc<Mutex<ViewportCache>>,
) {
    let graph = PathBuilder::new()
        .src(CacheReader {
            cache_dir: cache_dir.to_owned(),
            mip_level: mip,
            tile_size: TILE_SIZE,
            image_width: img_w,
            image_height: img_h,
            tile_range: Some(range),
        })
        .sink(ViewportCacheSink)
        .mark_output(0)
        .compile();

    thread::spawn(move || {
        let Ok(pipeline) = Pipeline::compile(&graph, None) else {
            tracing::warn!("[pixors] fetch_mip {mip}: compile failed (cache not on disk yet?)");
            return;
        };
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] fetch_mip {mip} error: {e}");
        }
    });
}

pub fn export_file(path: &Path, config: EncoderConfig) -> Result<(), String> {
    let img = Image::open(path).map_err(|e| e.to_string())?;
    let w = img.desc.width;
    let h = img.desc.height;

    tracing::info!("[pixors] export: {}×{} to {}", w, h, path.display());

    let stream = Arc::new(Mutex::new(Some(
        img.open_page(0).map_err(|e| e.to_string())?
    )));

    let encoder_sink = match &config {
        EncoderConfig::Png(png_cfg) => SinkNode::PngEncoderV2(
            pixors_executor::sink::png_encoder_v2::PngEncoderV2 {
                path: path.to_path_buf(),
                config: png_cfg.clone(),
                dpi: img.desc.dpi,
                icc_profile: img.desc.icc_profile.clone(),
            },
        ),
        EncoderConfig::Tiff(tiff_cfg) => SinkNode::TiffEncoder(
            pixors_executor::sink::tiff_encoder::TiffEncoderStage {
                path: path.to_path_buf(),
                config: tiff_cfg.clone(),
                dpi: img.desc.dpi,
                icc_profile: img.desc.icc_profile.clone(),
            },
        ),
    };

    let graph = PathBuilder::new()
        .src(ImageStreamSource { stream, image_height: img.desc.height })
        .data_xform(ScanLineToTile { tile_size: TILE_SIZE, image_width: w, image_height: h })
        .op(ColorConvert { target_format: PixelFormat::Rgba8, target_color_space: ColorSpace::SRGB, target_alpha: AlphaPolicy::Straight })
        .sink(encoder_sink)
        .mark_output(0)
        .compile();

    let (event_tx, event_rx) = sync_channel::<PipelineEvent>(64);
    let pipeline = Pipeline::compile(&graph, Some(event_tx.clone())).map_err(|e| e.to_string())?;
    let join_handle = thread::spawn(move || {
        if let Err(e) = pipeline.run(None) {
            tracing::error!("[pixors] export pipeline error: {e}");
            let _ = event_tx.send(PipelineEvent::Error(e.to_string()));
        }
    });

    while let Ok(event) = event_rx.recv() {
        match event {
            PipelineEvent::Done => break,
            PipelineEvent::Error(e) => {
                let _ = join_handle.join();
                return Err(e);
            }
            _ => {}
        }
    }

    let _ = join_handle.join();
    tracing::info!("[pixors] export complete: {}", path.display());
    Ok(())
}
