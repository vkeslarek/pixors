use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use crate::egraph::item::Item;
use crate::egraph::runner::SinkRunner;
use crate::error::Error;
use crate::storage::Buffer;

pub struct PngEncoderRunner {
    path: PathBuf,
    rows: HashMap<u32, Vec<u8>>,
    image_width: u32,
    image_height: u32,
    bpp: u8,
}

impl PngEncoderRunner {
    pub fn new(path: PathBuf) -> Self {
        Self { path, rows: HashMap::new(), image_width: 0, image_height: 0, bpp: 0 }
    }
}

impl SinkRunner for PngEncoderRunner {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let scanline = match item {
            Item::ScanLine(s) => s,
            _ => return Err(Error::internal("expected ScanLine")),
        };
        let data = match scanline.data {
            Buffer::Cpu(v) => v,
            Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
        };
        self.image_width = self.image_width.max(scanline.width);
        self.image_height = self.image_height.max(scanline.y + 1);
        self.bpp = scanline.meta.format.bytes_per_pixel() as u8;
        self.rows.insert(scanline.y, data);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        let bpp = self.bpp as usize;
        if bpp == 0 { return Err(Error::internal("no data received")); }
        let iw = self.image_width as usize;
        let ih = self.image_height as usize;
        let mut image = vec![0u8; iw * ih * bpp];

        for y in 0..self.image_height {
            if let Some(row) = self.rows.get(&y) {
                let dst_start = y as usize * iw * bpp;
                let len = row.len().min(image.len() - dst_start);
                image[dst_start..dst_start + len].copy_from_slice(&row[..len]);
            }
        }

        let file = File::create(&self.path)?;
        let w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, self.image_width, self.image_height);
        encoder.set_color(match bpp { 1=>png::ColorType::Grayscale, 2=>png::ColorType::GrayscaleAlpha, 3=>png::ColorType::Rgb, _=>png::ColorType::Rgba });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| Error::Png(e.to_string()))?;
        writer.write_image_data(&image).map_err(|e| Error::Png(e.to_string()))?;
        writer.finish().map_err(|e| Error::Png(e.to_string()))?;
        Ok(())
    }
}
