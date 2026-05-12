use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};
use webp::Encoder;

use crate::codec::EncoderConfig;

static WE_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile", kind: DataKind::Tile,
}];
static WE_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(WE_INPUTS),
};

#[derive(Debug)]
pub struct WebPEncoderStage {
    path: PathBuf,
    quality: f32,
    buffer: Vec<u8>,
    img_w: u32,
    img_h: u32,
    done: bool,
}

impl WebPEncoderStage {
    pub fn new(path: PathBuf, config: &EncoderConfig) -> Self {
        match config {
            EncoderConfig::WebP(cfg) => Self {
                path, quality: if cfg.lossless { 100.0 } else { cfg.quality },
                buffer: Vec::new(), img_w: 0, img_h: 0, done: false,
            },
            _ => panic!("wrong config type for WebPEncoderStage"),
        }
    }
}

impl Consumer for WebPEncoderStage {
    fn kind(&self) -> &'static str { "webp_encoder" }
    fn in_ports(&self) -> &'static InPortSpecification { &WE_IN_PORTS }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        if self.done { return Ok(()); }
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("WebPEncoderStage requires CPU tiles")),
        };
        self.img_w = self.img_w.max(tile.coord.px + tile.coord.width);
        self.img_h = self.img_h.max(tile.coord.py + tile.coord.height);
        let iw = self.img_w as usize;
        let ih = self.img_h as usize;
        let row_stride = iw * 4; // RGBA
        if self.buffer.len() < ih * row_stride {
            self.buffer.resize(ih * row_stride, 0);
        }
        // Copy RGBA pixels row by row
        for row in 0..tile.coord.height as usize {
            let src_start = row * tile.coord.width as usize * 4;
            if src_start + tile.coord.width as usize * 4 > data.len() { break; }
            let dst_row = (tile.coord.py as usize + row) * row_stride;
            let dst_start = dst_row + tile.coord.px as usize * 4;
            let len = (tile.coord.width as usize * 4).min(self.buffer.len() - dst_start);
            self.buffer[dst_start..dst_start + len].copy_from_slice(&data[src_start..src_start + len]);
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        if self.done { return Ok(()); }
        self.done = true;
        if self.img_w == 0 || self.img_h == 0 {
            return Err(Error::internal("no image data for WebP export"));
        }
        let enc = Encoder::from_rgba(&self.buffer, self.img_w, self.img_h);
        let webp_data = enc.encode(self.quality);

        let file = File::create(&self.path)
            .map_err(|e| Error::internal(format!("WebP create: {e}")))?;
        let mut w = BufWriter::new(file);
        std::io::Write::write_all(&mut w, &webp_data)
            .map_err(|e| Error::internal(format!("WebP write: {e}")))?;
        Ok(())
    }
}
