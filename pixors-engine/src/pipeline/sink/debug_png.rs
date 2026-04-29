use crate::convert::ColorConversion;
use crate::image::{Tile, TileCoord};
use crate::pipeline::sink::Sink;
use crate::pixel::{AlphaPolicy, Rgba};
use half::f16;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

struct TileSlot {
    coord: TileCoord,
    pixels: Vec<[u8; 4]>,
}

/// Debug sink that assembles all tiles into a PNG file.
/// Useful for visually verifying pipeline output.
pub struct DebugPngSink {
    path: PathBuf,
    color_conv: ColorConversion,
    image_w: u32,
    image_h: u32,
    tiles: Mutex<HashMap<(u32, u32), TileSlot>>,
}

impl DebugPngSink {
    pub fn new(path: impl Into<PathBuf>, color_conv: ColorConversion, image_w: u32, image_h: u32) -> Self {
        Self {
            path: path.into(),
            color_conv,
            image_w,
            image_h,
            tiles: Mutex::new(HashMap::new()),
        }
    }
}

impl Sink for DebugPngSink {
    type Item = Tile<Rgba<f16>>;

    fn consume(&self, item: Self::Item) -> Result<(), crate::error::Error> {
        let pixels: Vec<[u8; 4]> = self.color_conv.convert_pixels(
            &item.data,
            AlphaPolicy::Straight,
        );
        self.tiles.lock().unwrap().insert((item.coord.px, item.coord.py), TileSlot {
            coord: item.coord,
            pixels,
        });
        Ok(())
    }

    fn finish(&self) {
        let tiles = self.tiles.lock().unwrap();
        let img_w = self.image_w as usize;
        let img_h = self.image_h as usize;

        if img_w == 0 || img_h == 0 {
            tracing::error!("[DebugPngSink] Zero-size image, skipping PNG write");
            return;
        }

        let mut image = vec![0u8; img_w * img_h * 4];

        for (_key, slot) in tiles.iter() {
            let px = slot.coord.px as usize;
            let py = slot.coord.py as usize;
            let tw = slot.coord.width as usize;
            let th = slot.coord.height as usize;

            for row in 0..th {
                let src_off = row * tw;
                let dst_off = ((py + row) * img_w + px) * 4;
                let len = tw * 4;
                let src_bytes = bytemuck::cast_slice::<[u8; 4], u8>(&slot.pixels[src_off..src_off + tw]);
                if dst_off + len <= image.len() {
                    image[dst_off..dst_off + len].copy_from_slice(src_bytes);
                }
            }
        }

        let tile_count = tiles.len();
        drop(tiles);

        match std::fs::File::create(&self.path) {
            Ok(file) => {
                let mut encoder = png::Encoder::new(file, self.image_w, self.image_h);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                match encoder.write_header() {
                    Ok(mut writer) => {
                        if let Err(e) = writer.write_image_data(&image) {
                            tracing::error!("[DebugPngSink] Failed to write image data: {}", e);
                        } else {
                            tracing::info!(
                                "[DebugPngSink] Wrote {} tiles → {} ({}x{})",
                                tile_count,
                                self.path.display(),
                                self.image_w,
                                self.image_h
                            );
                        }
                    }
                    Err(e) => tracing::error!("[DebugPngSink] Failed to write PNG header: {}", e),
                }
            }
            Err(e) => tracing::error!("[DebugPngSink] Failed to create file {}: {}", self.path.display(), e),
        }
    }
}
