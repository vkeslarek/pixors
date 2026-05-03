use pixors_engine::color::ColorSpace;
use pixors_engine::color::ColorConversion;
use pixors_engine::image::Tile;
use pixors_engine::pixel::Rgba;
use crate::pipeline::sink::Sink;
use pixors_engine::storage::WorkingWriter;
use half::f16;
use std::sync::Arc;

pub struct WorkingSink {
    store: Arc<WorkingWriter>,
    color_conv: ColorConversion,
}

impl WorkingSink {
    pub fn new(store: Arc<WorkingWriter>, color_conv: ColorConversion) -> Self {
        Self { store, color_conv }
    }
}

// ── Sink (Tile-based API) ────────────────────────────────────────────

impl Sink for WorkingSink {
    type Item = Tile<Rgba<f16>>;

    fn consume(&self, item: Self::Item) -> Result<(), pixors_engine::error::Error> {
        let converted = self.color_conv.convert_pixels(
            &item.data,
            pixors_engine::pixel::AlphaPolicy::PremultiplyOnPack,
        );
        let tile = Tile::with_color_space(item.coord, converted, ColorSpace::ACES_CG);

        self.store.write_tile_f16(&tile).map_err(|e| {
            tracing::error!("WorkingSink: write failed: {}", e);
            e
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixors_engine::image::TileCoord;

    #[test]
    fn workingsink_consume_writes_and_reads_back() {
        let dir = std::env::temp_dir().join("pixors_test_wsink");
        std::fs::create_dir_all(&dir).unwrap();
        let writer = Arc::new(
            WorkingWriter::new(dir.clone(), 256, 256, 256).unwrap(),
        );
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::ACES_CG).unwrap();
        let sink = WorkingSink::new(Arc::clone(&writer), conv);

        let coord = TileCoord::from_xywh(0, 0, 0, 256, 256);
        let pixels: Vec<Rgba<f16>> = (0..65536)
            .map(|i| {
                let v = (i % 256) as f32 / 255.0;
                Rgba {
                    r: f16::from_f32(v),
                    g: f16::from_f32(1.0 - v),
                    b: f16::from_f32(0.5),
                    a: f16::ONE,
                }
            })
            .collect();
        let tile = Tile::new(coord, pixels);
        sink.consume(tile).unwrap();

        let read = writer.read_tile(TileCoord::from_xywh(0, 0, 0, 256, 256)).unwrap().unwrap();
        assert_eq!(read.data.len(), 65536);
        let px = &read.data[0];
        assert!((px.r.to_f32() - 0.0).abs() < 0.01, "first pixel r should be ~0");
        assert!((px.g.to_f32() - 1.0).abs() < 0.01, "first pixel g should be ~1");
        assert!((px.a.to_f32() - 1.0).abs() < 0.01, "alpha should be 1.0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workingsink_preserves_pixel_count() {
        let dir = std::env::temp_dir().join("pixors_test_wsink2");
        std::fs::create_dir_all(&dir).unwrap();
        let writer = Arc::new(
            WorkingWriter::new(dir.clone(), 64, 128, 128).unwrap(),
        );
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::ACES_CG).unwrap();
        let sink = WorkingSink::new(Arc::clone(&writer), conv);

        let coord = TileCoord::from_xywh(0, 0, 0, 64, 64);
        let pixels: Vec<Rgba<f16>> = vec![Rgba {
            r: f16::from_f32(0.3),
            g: f16::from_f32(0.6),
            b: f16::from_f32(0.1),
            a: f16::ONE,
        }; 4096];
        let tile = Tile::new(coord, pixels);
        sink.consume(tile).unwrap();

        let read = writer.read_tile(TileCoord::from_xywh(0, 0, 0, 64, 64)).unwrap().unwrap();
        assert_eq!(read.data.len(), 4096);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
