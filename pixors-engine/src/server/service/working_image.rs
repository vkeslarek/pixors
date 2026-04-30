//! WorkingImage — manages the image pipeline, layers, mips, and compositing.
//! Extracted from TabData so image state can be swapped independently of tab identity.

use crate::color::ColorSpace;
use crate::composite::{self, CompositeRequest, LayerView};
use crate::convert::ColorConversion;
use crate::error::Error;
use crate::image::{BlendMode, MipPyramid, Tile, TileCoord, TileGrid};
use crate::pipeline::operation::Operation;
use crate::pipeline::sink::viewport::{Viewport, ViewportSink};
use crate::pipeline::sink::working::WorkingSink;
use crate::pipeline::sink::Sink;
use crate::pixel::Rgba;
use crate::storage::writer::WorkingWriter;
use half::f16;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use uuid::Uuid;

pub struct LayerSlot {
    pub id: Uuid,
    pub tile_store: Arc<WorkingWriter>,
    pub mip_pyramid: MipPyramid,
    pub mip_base_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub offset: (i32, i32),
    pub opacity: f32,
    pub visible: bool,
    pub blend_mode: BlendMode,
    pub viewport: Arc<Viewport>,
    pub disk_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LayerSlot {
    fn drop(&mut self) {
        if let Some(h) = self.disk_handle.take() {
            let _ = h.join();
        }
    }
}

pub struct WorkingImage {
    pub has_image: bool,
    pub color_conversion: Option<ColorConversion>,
    pub tile_size: u32,
    pub layers: Vec<LayerSlot>,
    pub doc_width: u32,
    pub doc_height: u32,
    pub doc_origin: (i32, i32),
    pub doc_grid: Option<TileGrid>,
    pub is_generating_mips: Arc<AtomicBool>,
    base_dir: PathBuf,
}

impl WorkingImage {
    pub fn new(tile_size: u32, base_dir: PathBuf) -> Self {
        Self {
            has_image: false, color_conversion: None, tile_size,
            layers: Vec::new(), doc_width: 0, doc_height: 0, doc_origin: (0, 0),
            doc_grid: None,
            is_generating_mips: Arc::new(AtomicBool::new(false)),
            base_dir,
        }
    }

    pub fn base_dir(&self) -> &Path { &self.base_dir }

    pub fn recompute_doc_bounds(&mut self) {
        let (w, h, ox, oy) = if self.layers.is_empty() {
            (0, 0, 0, 0)
        } else {
            let mut min_x = i32::MAX; let mut min_y = i32::MAX;
            let mut max_x = i32::MIN; let mut max_y = i32::MIN;
            for l in &self.layers {
                min_x = min_x.min(l.offset.0); min_y = min_y.min(l.offset.1);
                max_x = max_x.max(l.offset.0 + l.width as i32);
                max_y = max_y.max(l.offset.1 + l.height as i32);
            }
            if min_x == i32::MAX { (0, 0, 0, 0) }
            else { ((max_x - min_x).max(0) as u32, (max_y - min_y).max(0) as u32, min_x, min_y) }
        };
        self.doc_width = w; self.doc_height = h; self.doc_origin = (ox, oy);
        self.doc_grid = if w > 0 && h > 0 { Some(TileGrid::new(w, h, self.tile_size)) } else { None };
    }

    pub fn composition_sig(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        for layer in &self.layers {
            layer.id.hash(&mut h); layer.visible.hash(&mut h);
            layer.opacity.to_bits().hash(&mut h);
            layer.blend_mode.hash(&mut h);
            layer.offset.0.hash(&mut h); layer.offset.1.hash(&mut h);
        }
        h.finish()
    }

    pub fn is_mip_ready(&self, mip_level: usize) -> bool {
        if mip_level == 0 { return true; }
        self.layers.iter().all(|l| l.mip_pyramid.level(mip_level).map(|lv| lv.generated).unwrap_or(false))
    }

    pub fn is_display_mip_ready(&self, mip_level: usize) -> bool {
        self.layers.iter().any(|l| {
            l.viewport.get(mip_level as u32, TileCoord::new(
                mip_level as u32, 0, 0, self.tile_size,
                (self.doc_width >> mip_level).max(1), (self.doc_height >> mip_level).max(1),
            )).is_some()
        })
    }

    pub fn layer_views_for_mip(&self, mip_level: usize) -> Vec<LayerView<'_>> {
        self.layers.iter().filter(|l| l.visible).map(|l| {
            let actual_mip = mip_level as u32;
            let (w, h) = ((l.width >> mip_level).max(1), (l.height >> mip_level).max(1));
            let comp_offset = (
                (l.offset.0 - self.doc_origin.0) >> actual_mip,
                (l.offset.1 - self.doc_origin.1) >> actual_mip,
            );
            LayerView { id: l.id, store: &l.tile_store, size: (w, h), offset: comp_offset, opacity: l.opacity, blend: l.blend_mode, mip_level: actual_mip }
        }).collect()
    }

    pub fn image_info(&self) -> Option<(u32, u32)> {
        if self.has_image { Some((self.doc_width, self.doc_height)) } else { None }
    }

    pub fn tile_grid(&self) -> Option<&TileGrid> { self.doc_grid.as_ref() }

    pub fn tile_grid_for_mip(&self, mip_level: usize) -> Option<TileGrid> {
        if mip_level == 0 { self.doc_grid.clone() }
        else {
            let lvl = mip_level as u32;
            Some(TileGrid::new((self.doc_width >> lvl).max(1), (self.doc_height >> lvl).max(1), self.tile_size))
        }
    }

    pub fn close_image(&mut self) {
        self.color_conversion = None; self.has_image = false;
        self.layers.clear(); self.doc_width = 0; self.doc_height = 0; self.doc_grid = None;
    }

    pub async fn open_image_v2(
        &mut self,
        path: impl AsRef<Path>,
        vp_cb: Option<Arc<dyn Fn(u32, crate::image::TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
    ) -> Result<(), Error> {
        self.close_image();
        let path = path.as_ref();
        self.color_conversion = Some(ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB)?);
        self.has_image = true;
        let reader = crate::io::all_readers().iter().find(|r| r.can_handle(path)).copied()
            .ok_or_else(|| Error::unsupported_sample_type("No reader for file"))?;
        let info = reader.read_document_info(path)?;
        for layer_idx in 0..info.layer_count {
            let meta = reader.read_layer_metadata(path, layer_idx)?;
            self.add_layer_pipeline(path, meta.desc.width, meta.desc.height, meta.desc.color_space, meta.offset, vp_cb.clone())?;
        }
        self.recompute_doc_bounds();
        Ok(())
    }

    fn add_layer_pipeline(
        &mut self, path: &Path, w: u32, h: u32, src_cs: ColorSpace,
        offset: (i32, i32),
        vp_cb: Option<Arc<dyn Fn(u32, crate::image::TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
    ) -> Result<(), Error> {
        let mip_base = self.base_dir.join(format!("layer_{}_mips", self.layers.len()));
        std::fs::create_dir_all(&mip_base)?;
        let mip = MipPyramid::new(w, h, self.tile_size, mip_base.clone())?;
        let store_path = self.base_dir.join(format!("layer_{}", self.layers.len()));
        let store = Arc::new(WorkingWriter::new(store_path, self.tile_size, w, h)?);
        let mut viewport = Viewport::new();
        viewport.on_tile_added = vp_cb;
        let viewport = Arc::new(viewport);

        use crate::pipeline::job::Job;
        use crate::pipeline::source::file::FileImageSource;
        use crate::pipeline::operation::color::ColorConvertOperation;
        use crate::pipeline::operation::mip::MipOp;
        use crate::pixel::AlphaPolicy;

        let max_mip = (w.max(h) as f32).log2().ceil() as u32;
        let mut branches = Job::from_source(FileImageSource::new(path, self.tile_size))
            .then(ColorConvertOperation::with_conv(src_cs.converter_to(ColorSpace::ACES_CG)?, AlphaPolicy::PremultiplyOnPack))
            .then(MipOp::new(self.tile_size, max_mip, w, h))
            .split(2);
        let br1 = branches.remove(0);
        let br2 = branches.remove(0);
        let wk_job = br1.sink(WorkingSink::new(Arc::clone(&store), ColorSpace::ACES_CG.converter_to(ColorSpace::ACES_CG).unwrap()));
        let vp_job = br2.sink(ViewportSink::new(Arc::clone(&viewport), ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap()));
        let wk_handle = std::thread::spawn(move || { wk_job.join(); });
        std::thread::spawn(move || { vp_job.join(); });

        self.layers.push(LayerSlot {
            id: Uuid::new_v4(), tile_store: store, mip_pyramid: mip, mip_base_dir: mip_base,
            width: w, height: h, offset, opacity: 1.0, visible: true, blend_mode: BlendMode::Normal,
            viewport, disk_handle: Some(wk_handle),
        });
        Ok(())
    }

    pub async fn get_tile_rgba8(&self, tile: TileCoord, mip_level: usize) -> Result<Arc<Vec<u8>>, Error> {
        let mip = mip_level as u32;
        let visible_count = self.layers.iter().filter(|l| l.visible).count();
        if visible_count == 1
            && let Some(layer) = self.layers.iter().find(|l| l.visible)
            && let Some(rgba8) = layer.viewport.get(mip, tile)
        { return Ok(rgba8); }
        let views = self.layer_views_for_mip(mip_level);
        let composed: Vec<Rgba<f16>> = composite::composite_tile(&CompositeRequest { layers: &views, coord: tile, tile_size: self.tile_size })?;
        let conv = self.color_conversion.as_ref().ok_or_else(|| Error::invalid_param("Color conversion not initialized"))?;
        Ok(crate::image::Tile::new(tile, composed).to_srgb_u8(conv).data)
    }

    pub async fn ensure_mip_level(&mut self, zoom: f32) -> Result<(), Error> {
        let level_idx = MipPyramid::level_for_zoom(zoom);
        if level_idx == 0 { return Ok(()); }
        for layer in &mut self.layers { if let Some(h) = layer.disk_handle.take() { let _ = h.join(); } }
        let is_gen = self.is_generating_mips.clone();
        for layer in &mut self.layers {
            if layer.mip_pyramid.level(level_idx).map(|l| l.generated).unwrap_or(false) { continue; }
            if is_gen.swap(true, std::sync::atomic::Ordering::SeqCst) { return Ok(()); }
            let ts = layer.tile_store.tile_size();
            let iw = layer.tile_store.image_width();
            let ih = layer.tile_store.image_height();
            let mip0_path = layer.tile_store.base_dir();
            let mip_base = layer.mip_base_dir.clone();
            let mip0_view = WorkingWriter::open(mip0_path, ts, iw, ih)?;
            let regenerated = tokio::task::spawn_blocking(move || MipPyramid::generate_from_mip0(&mip0_view, &mip_base)).await
                .map_err(|e| Error::Io(std::io::Error::other(e)))?;
            match regenerated {
                Ok(p) => layer.mip_pyramid.replace_levels(p.into_levels()),
                Err(e) => { is_gen.store(false, std::sync::atomic::Ordering::SeqCst); return Err(e); }
            }
            is_gen.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    pub fn apply_gaussian_blur(&mut self, radius: u32) -> Result<(), Error> {
        tracing::info!("[WorkingImage] gaussian blur radius={} layers={}", radius, self.layers.len());
        use crate::pipeline::job::Job;
        use crate::pipeline::operation::blur::BoxBlurOp;
        use crate::pipeline::operation::neighborhood::NeighborhoodAccumOp;
        use crate::pipeline::source::working::WorkSource;

        let conv = self.color_conversion.as_ref()
            .ok_or_else(|| Error::invalid_param("No color conversion"))?.clone();

        let tile_radius = radius.div_ceil(self.tile_size);
        for layer in &self.layers {
            let source = WorkSource::new(
                Arc::clone(&layer.tile_store), self.tile_size, layer.width, layer.height);
            let vp_sink = ViewportSink::new(Arc::clone(&layer.viewport), conv.clone());

            Job::from_source(source)
                .then(NeighborhoodAccumOp::<Rgba<f16>>::new(tile_radius, layer.width, layer.height, self.tile_size))
                .then(BoxBlurOp::<Rgba<f16>>::new(radius, radius))
                .sink(vp_sink)
                .join();
        }
        Ok(())
    }

    pub fn image_width(&self) -> u32 { self.doc_width }
    pub fn image_height(&self) -> u32 { self.doc_height }
}
