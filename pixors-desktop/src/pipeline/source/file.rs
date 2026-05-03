use pixors_engine::error::Error;
use pixors_engine::image::{Tile, TileCoord, TileGrid};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::source::Source;
use pixors_engine::pixel::Rgba;
use half::f16;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Reads an image file (PNG, TIFF) tile-by-tile and emits typed tiles.
pub struct FileImageSource {
    path: PathBuf,
    tile_size: u32,
}

impl FileImageSource {
    pub fn new(path: impl AsRef<Path>, tile_size: u32) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            tile_size,
        }
    }
}

impl Source for FileImageSource {
    type Item = Tile<Rgba<f16>>;

    fn run(self, emit: &mut Emitter<Self::Item>, cancel: Arc<AtomicBool>) {
        if let Err(e) = Self::run_impl(self.path, self.tile_size, emit, cancel) {
            tracing::error!("FileImageSource failed: {}", e);
        }
    }

    fn total(&self) -> Option<u32> {
        // Estimate from file — open synchronously to get dimensions
        let reader = pixors_engine::io::all_readers()
            .iter()
            .find(|r| r.can_handle(&self.path))
            .copied()?;
        let meta = reader.read_layer_metadata(&self.path, 0).ok()?;
        let grid = TileGrid::new(meta.desc.width, meta.desc.height, self.tile_size);
        Some(grid.tile_count() as u32)
    }
}

impl FileImageSource {
    fn run_impl(
        path: PathBuf,
        tile_size: u32,
        emit: &mut Emitter<Tile<Rgba<f16>>>,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let reader = pixors_engine::io::all_readers()
            .iter()
            .find(|r| r.can_handle(&path))
            .copied()
            .ok_or_else(|| Error::unsupported_sample_type("No reader for file"))?;

        let info = reader.read_document_info(&path)?;

        for layer_idx in 0..info.layer_count {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let meta = reader.read_layer_metadata(&path, layer_idx)?;
            let w = meta.desc.width;
            let h = meta.desc.height;
            let channels = meta.desc.planes.len();
            let tiles_x = w.div_ceil(tile_size);

            let (layer_tx, layer_rx) = std::sync::mpsc::channel::<Vec<u8>>();
            let path_c = path.clone();

            struct RawWriter(std::sync::mpsc::Sender<Vec<u8>>);
            impl pixors_engine::storage::writer::TileWriter<u8> for RawWriter {
                fn write_tile(&self, _coord: TileCoord, pixels: &[u8]) -> Result<(), Error> {
                    self.0.send(pixels.to_vec()).ok();
                    Ok(())
                }
                fn name(&self) -> &'static str {
                    "RawWriter"
                }
            }

            std::thread::spawn(move || {
                let _ =
                    reader.stream_tiles(&path_c, tile_size, &RawWriter(layer_tx), layer_idx, None);
            });

            let mut emitted = 0u32;
            for raw in layer_rx {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let tx_tile = emitted % tiles_x;
                let ty_tile = emitted / tiles_x;
                let coord = TileCoord::new(0, tx_tile, ty_tile, tile_size, w, h);

                let pixels: Vec<Rgba<f16>> = match channels {
                    4 => raw
                        .chunks_exact(4)
                        .map(|c| Rgba {
                            r: f16::from_f32(c[0] as f32 / 255.0),
                            g: f16::from_f32(c[1] as f32 / 255.0),
                            b: f16::from_f32(c[2] as f32 / 255.0),
                            a: f16::from_f32(c[3] as f32 / 255.0),
                        })
                        .collect(),
                    3 => raw
                        .chunks_exact(3)
                        .map(|c| Rgba {
                            r: f16::from_f32(c[0] as f32 / 255.0),
                            g: f16::from_f32(c[1] as f32 / 255.0),
                            b: f16::from_f32(c[2] as f32 / 255.0),
                            a: f16::ONE,
                        })
                        .collect(),
                    1 => raw
                        .chunks_exact(1)
                        .map(|c| {
                            let v = f16::from_f32(c[0] as f32 / 255.0);
                            Rgba::new(v, v, v, f16::ONE)
                        })
                        .collect(),
                    _ => raw
                        .chunks_exact(4)
                        .map(|c| Rgba {
                            r: f16::from_f32(c[0] as f32 / 255.0),
                            g: f16::from_f32(c[1] as f32 / 255.0),
                            b: f16::from_f32(c[2] as f32 / 255.0),
                            a: f16::from_f32(c[3] as f32 / 255.0),
                        })
                        .collect(),
                };

                if emitted <= 2 {
                    tracing::debug!(
                        "[FileImageSource] tile {}: tx={} ty={} px={} py={} w={} h={} pixels={} channels={}",
                        emitted,
                        coord.tx,
                        coord.ty,
                        coord.px,
                        coord.py,
                        coord.width,
                        coord.height,
                        pixels.len(),
                        channels
                    );
                }

                emit.emit(Tile::new(coord, pixels));
                emitted += 1;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::emitter::Emitter;

    fn make_test_rgb_png(path: &std::path::Path, w: u32, h: u32, colors: &[(u8, u8, u8)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();

        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for &(r, g, b) in colors.iter().cycle().take((w * h) as usize) {
            data.push(r);
            data.push(g);
            data.push(b);
        }
        writer.write_image_data(&data).unwrap();
    }

    #[test]
    fn file_source_emits_correct_tile_count() {
        let dir = std::env::temp_dir().join("pixors_test_file_source");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_rgb.png");

        let w = 8u32;
        let h = 6u32;
        let colors = [(255u8, 0, 0), (0, 255, 0), (0, 0, 255)];
        make_test_rgb_png(&path, w, h, &colors);

        let source = FileImageSource::new(&path, 4);
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        let mut emit = Emitter::new(tx);
        let cancel = Arc::new(AtomicBool::new(false));

        source.run(&mut emit, cancel);
        drop(emit);

        let tiles: Vec<Tile<Rgba<f16>>> = rx.iter().collect();
        let expected = (w.div_ceil(4) * h.div_ceil(4)) as usize;
        assert_eq!(
            tiles.len(),
            expected,
            "tile count for {}x{} with tile_size=4 should be {}",
            w,
            h,
            expected
        );

        for tile in &tiles {
            let px_count = tile.coord.pixel_count();
            assert!(px_count > 0, "tile should have non-zero pixel count");
            assert_eq!(
                tile.data.len(),
                px_count,
                "tile data length should match pixel count"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_source_pixel_values_are_in_range() {
        let dir = std::env::temp_dir().join("pixors_test_fs_values");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("small.png");

        make_test_rgb_png(&path, 4, 4, &[(100, 150, 200)]);

        let source = FileImageSource::new(&path, 4);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut emit = Emitter::new(tx);
        let cancel = Arc::new(AtomicBool::new(false));

        source.run(&mut emit, cancel);
        drop(emit);

        let tiles: Vec<Tile<Rgba<f16>>> = rx.iter().collect();
        assert_eq!(tiles.len(), 1);

        let tile = &tiles[0];
        assert_eq!(tile.data.len(), 16);
        let px = &tile.data[0];
        let r = px.r.to_f32();
        let g = px.g.to_f32();
        let b = px.b.to_f32();
        assert!(
            (r - 100.0 / 255.0).abs() < 0.02,
            "red {:.3} should be ~0.392",
            r
        );
        assert!(
            (g - 150.0 / 255.0).abs() < 0.02,
            "green {:.3} should be ~0.588",
            g
        );
        assert!(
            (b - 200.0 / 255.0).abs() < 0.02,
            "blue {:.3} should be ~0.784",
            b
        );
        assert!(
            (px.a.to_f32() - 1.0).abs() < 0.01,
            "alpha should be 1.0 for RGB PNG"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_source_total_returns_count() {
        let dir = std::env::temp_dir().join("pixors_test_fs_total");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("total.png");

        make_test_rgb_png(&path, 12, 12, &[(0, 128, 255)]);
        let source = FileImageSource::new(&path, 4);
        let total = source.total().unwrap();
        assert_eq!(total, 9, "12x12 with tile_size=4 → 3x3 tiles = 9");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
