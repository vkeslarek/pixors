use std::fs::File;
use std::io::{BufWriter, Cursor};
use std::path::PathBuf;

use mozjpeg::{ColorSpace as JpegColorSpace, Compress};

use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

use crate::codec::EncoderConfig;

static JE_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile", kind: DataKind::Tile,
}];
static JE_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(JE_INPUTS),
};

#[derive(Debug)]
pub struct JpegEncoderStage {
    path: PathBuf,
    quality: u8,
    metadata: Option<PixelMeta>,
    buffer: Vec<u8>,
    img_w: u32,
    img_h: u32,
    done: bool,
}

impl JpegEncoderStage {
    pub fn new(path: PathBuf, config: &EncoderConfig) -> Self {
        match config {
            EncoderConfig::Jpeg(cfg) => Self {
                path, quality: cfg.quality.clamp(1, 100),
                metadata: None, buffer: Vec::new(), img_w: 0, img_h: 0, done: false,
            },
            _ => panic!("wrong config type for JpegEncoderStage"),
        }
    }
}

impl Consumer for JpegEncoderStage {
    fn kind(&self) -> &'static str { "jpeg_encoder" }
    fn in_ports(&self) -> &'static InPortSpecification { &JE_IN_PORTS }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        if self.done { return Ok(()); }
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("JpegEncoderStage requires CPU tiles")),
        };
        if self.metadata.is_none() { self.metadata = Some(tile.meta); }
        self.img_w = self.img_w.max(tile.coord.px + tile.coord.width);
        self.img_h = self.img_h.max(tile.coord.py + tile.coord.height);
        let iw = self.img_w as usize;
        let ih = self.img_h as usize;
        let row_stride = iw * 3;
        if self.buffer.len() < ih * row_stride {
            self.buffer.resize(ih * row_stride, 0);
        }
        for row in 0..tile.coord.height as usize {
            let src_start = row * tile.coord.width as usize * 4;
            if src_start + tile.coord.width as usize * 4 > data.len() { break; }
            let dst_row = (tile.coord.py as usize + row) * row_stride;
            let dst_col = tile.coord.px as usize * 3;
            let dst_start = dst_row + dst_col;
            for px in 0..tile.coord.width as usize {
                let si = src_start + px * 4;
                let di = dst_start + px * 3;
                if di + 2 < self.buffer.len() {
                    self.buffer[di] = data[si];
                    self.buffer[di + 1] = data[si + 1];
                    self.buffer[di + 2] = data[si + 2];
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        if self.done { return Ok(()); }
        self.done = true;
        if self.img_w == 0 || self.img_h == 0 {
            return Err(Error::internal("no image data for JPEG export"));
        }
        let mut comp = Compress::new(JpegColorSpace::JCS_RGB);
        comp.set_size(self.img_w as usize, self.img_h as usize);
        comp.set_quality(self.quality as f32);
        let mut started = comp
            .start_compress(Cursor::new(Vec::new()))
            .map_err(|e| Error::internal(format!("JPEG start: {e}")))?;

        let row_stride = self.img_w as usize * 3;
        for y in 0..self.img_h as usize {
            let start = y * row_stride;
            let end = (start + row_stride).min(self.buffer.len());
            started.write_scanlines(&self.buffer[start..end])
                .map_err(|e| Error::internal(format!("JPEG write scanline: {e}")))?;
        }
        let jpeg_data = started
            .finish()
            .map_err(|e| Error::internal(format!("JPEG finish: {e}")))?
            .into_inner();

        let file = File::create(&self.path)
            .map_err(|e| Error::internal(format!("JPEG create: {e}")))?;
        let mut w = BufWriter::new(file);
        std::io::Write::write_all(&mut w, &jpeg_data)
            .map_err(|e| Error::internal(format!("JPEG write: {e}")))?;
        Ok(())
    }
}
