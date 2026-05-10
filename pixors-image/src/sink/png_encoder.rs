use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;

use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};
use pixors_engine::error::Error;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::debug_stopwatch;

static PE_INPUTS: &[PortDeclaration] = &[PortDeclaration { name: "scanline", kind: DataKind::ScanLine }];
static PE_IN_PORTS: InPortSpecification = InPortSpecification { ports: PortGroup::Fixed(PE_INPUTS) };

#[derive(Debug, Clone)]
pub struct PngEncoder {
    pub path: PathBuf,
    rows: HashMap<u32, Vec<u8>>,
    image_width: u32,
    image_height: u32,
    bpp: u8,
}

impl Consumer for PngEncoder {
    fn kind(&self) -> &'static str { "png_encoder" }
    fn in_ports(&self) -> &'static InPortSpecification { &PE_IN_PORTS }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("png_encoder:consume");
        let scanline = pixors_engine::stage::ProcessorContext::take_scanline(item)?;
        let data: Vec<u8> = match scanline.data {
            Buffer::Cpu(v) => match Arc::try_unwrap(v) { Ok(o) => o, Err(s) => (*s).clone() },
            Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
        };
        self.image_width = self.image_width.max(scanline.width);
        self.image_height = self.image_height.max(scanline.y + 1);
        self.bpp = scanline.meta.format.bytes_per_pixel() as u8;
        self.rows.insert(scanline.y, data);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        let _sw = debug_stopwatch!("png_encoder:finish");
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
        encoder.set_color(match bpp { 1 => png::ColorType::Grayscale, 2 => png::ColorType::GrayscaleAlpha, 3 => png::ColorType::Rgb, _ => png::ColorType::Rgba });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| Error::Png(e.to_string()))?;
        writer.write_image_data(&image).map_err(|e| Error::Png(e.to_string()))?;
        writer.finish().map_err(|e| Error::Png(e.to_string()))?;
        Ok(())
    }
}
