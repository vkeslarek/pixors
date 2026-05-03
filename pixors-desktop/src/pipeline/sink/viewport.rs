use pixors_engine::color::ColorConversion;
use pixors_engine::image::{Tile, TileCoord};
use pixors_engine::pixel::{AlphaPolicy, Rgba};
use crate::pipeline::sink::Sink;
use half::f16;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// Viewport — persistent tile cache in RAM. The server queries this for tiles.
// ═══════════════════════════════════════════════════════════════════════════

pub struct Viewport {
    tiles: RwLock<HashMap<(u32, TileCoord), Arc<Vec<u8>>>>,
    ready: AtomicBool,
    pub on_tile_added: Option<Arc<dyn Fn(u32, TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
}

impl Viewport {
    pub fn new() -> Self {
        Self { tiles: RwLock::new(HashMap::new()), ready: AtomicBool::new(false), on_tile_added: None }
    }

    pub fn put(&self, mip: u32, coord: TileCoord, data: Arc<Vec<u8>>) {
        self.tiles.write().insert((mip, coord), data.clone());
        if let Some(cb) = &self.on_tile_added { cb(mip, coord, data); }
    }

    pub fn get(&self, mip: u32, coord: TileCoord) -> Option<Arc<Vec<u8>>> {
        self.tiles.read().get(&(mip, coord)).cloned()
    }

    pub fn clear(&self) { self.tiles.write().clear(); }
    pub fn mark_ready(&self) { self.ready.store(true, Ordering::Release); }
    pub fn mark_stale(&self) { self.ready.store(false, Ordering::Release); }
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::Acquire) }
    pub fn tile_count(&self) -> usize { self.tiles.read().len() }
}

// ═══════════════════════════════════════════════════════════════════════════
// ViewportSink — Sink (new Tile API)
// ═══════════════════════════════════════════════════════════════════════════

pub struct ViewportSink {
    viewport: Arc<Viewport>,
    color_conv: ColorConversion,
}

impl ViewportSink {
    pub fn new(viewport: Arc<Viewport>, color_conv: ColorConversion) -> Self {
        Self { viewport, color_conv }
    }
}

impl Sink for ViewportSink {
    type Item = Tile<Rgba<f16>>;

    fn consume(&self, item: Self::Item) -> Result<(), pixors_engine::error::Error> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let c = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if c <= 5 || c % 100 == 0 {
            tracing::debug!(
                "[ViewportSink] consume #{} mip={} tx={} ty={}",
                c, item.coord.mip_level, item.coord.tx, item.coord.ty
            );
        }

        let pixels: Vec<[u8; 4]> = self.color_conv.convert_pixels(&item.data, AlphaPolicy::Straight);
        let bytes: Vec<u8> = bytemuck::cast_slice::<[u8; 4], u8>(&pixels).to_vec();
        let mip = item.coord.mip_level;
        self.viewport.put(mip, item.coord, Arc::new(bytes));
        Ok(())
    }

    fn finish(&self) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let total = COUNT.swap(0, Ordering::Relaxed);
        tracing::debug!("[ViewportSink] finish — total tiles consumed: {}", total);
        self.viewport.mark_ready();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixors_engine::color::ColorSpace;
    use pixors_engine::image::TileCoord;

    fn make_tile(r: f32, g: f32, b: f32, a: f32, w: u32, h: u32) -> Tile<Rgba<f16>> {
        let coord = TileCoord::from_xywh(0, 0, 0, w, h);
        let pixels = vec![Rgba {
            r: f16::from_f32(r),
            g: f16::from_f32(g),
            b: f16::from_f32(b),
            a: f16::from_f32(a),
        }; (w * h) as usize];
        Tile::new(coord, pixels)
    }

    fn round(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    #[test]
    fn viewportsink_consume_stores_in_viewport() {
        let vp = Arc::new(Viewport::new());
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let sink = ViewportSink::new(Arc::clone(&vp), conv);

        let tile = make_tile(0.5, 0.3, 0.2, 1.0, 2, 2);
        let coord = tile.coord;
        sink.consume(tile).unwrap();

        let stored = vp.get(0, coord).expect("tile should be in viewport");
        assert!(!stored.is_empty(), "stored data should not be empty");
        assert_eq!(stored.len(), (coord.width * coord.height * 4) as usize);
    }

    #[test]
    fn viewportsink_finish_marks_ready() {
        let vp = Arc::new(Viewport::new());
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let sink = ViewportSink::new(Arc::clone(&vp), conv);

        assert!(!vp.is_ready());
        sink.finish();
        assert!(vp.is_ready());
    }

    #[test]
    fn viewportsink_clear_resets_ready() {
        let vp = Arc::new(Viewport::new());
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let sink = ViewportSink::new(Arc::clone(&vp), conv);
        sink.finish();
        assert!(vp.is_ready());
        vp.mark_stale();
        assert!(!vp.is_ready());
    }

    #[test]
    fn viewportsink_opaque_tile_alpha_is_255() {
        let vp = Arc::new(Viewport::new());
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let sink = ViewportSink::new(Arc::clone(&vp), conv);

        let tile = make_tile(0.5, 0.5, 0.5, 1.0, 1, 1);
        let coord = tile.coord;
        sink.consume(tile).unwrap();

        let stored = vp.get(0, coord).unwrap();
        assert_eq!(stored[3], 255, "alpha should be 255 for opaque tile");
    }

    #[test]
    fn viewportsink_linear_acecsg_preserves_values() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();

        let white = Rgba {
            r: f16::from_f32(1.0),
            g: f16::from_f32(1.0),
            b: f16::from_f32(1.0),
            a: f16::ONE,
        };
        let coord = TileCoord::from_xywh(0, 0, 0, 1, 1);
        let tile = Tile::new(coord, vec![white]);

        let pixels: Vec<[u8; 4]> = conv.convert_pixels::<Rgba<f16>, [u8; 4]>(
            &tile.data,
            AlphaPolicy::Straight,
        );
        let out = &pixels[0];
        assert!(out[0] > 200, "white r should be bright: {}", out[0]);
        assert!(out[1] > 200, "white g should be bright: {}", out[1]);
        assert!(out[2] > 200, "white b should be bright: {}", out[2]);
        assert_eq!(out[3], 255, "white a should be 255");
    }
}
