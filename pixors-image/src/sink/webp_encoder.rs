use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};
use webp::Encoder;

use crate::codec::EncoderConfig;

static WE_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static WE_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(WE_INPUTS),
};

#[derive(Debug)]
pub struct WebPEncoderStage {
    path: PathBuf,
    quality: f32,
    tiles: HashMap<(u32, u32), (u32, u32, Vec<u8>)>,
    img_w: u32,
    img_h: u32,
    meta: Option<PixelMeta>,
    done: bool,
}

impl WebPEncoderStage {
    pub fn new(path: PathBuf, config: &EncoderConfig) -> Self {
        match config {
            EncoderConfig::WebP(cfg) => Self {
                path,
                quality: if cfg.lossless { 100.0 } else { cfg.quality },
                tiles: HashMap::new(),
                img_w: 0,
                img_h: 0,
                meta: None,
                done: false,
            },
            _ => panic!("wrong config type for WebPEncoderStage"),
        }
    }
}

impl Consumer for WebPEncoderStage {
    fn kind(&self) -> &'static str {
        "webp_encoder"
    }
    fn in_ports(&self) -> &'static InPortSpecification {
        &WE_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data: Vec<u8> = match &tile.data {
            Buffer::Cpu(v) => v.as_slice().to_vec(),
            Buffer::Gpu(_) => return Err(Error::internal("WebPEncoderStage requires CPU tiles")),
        };
        if self.meta.is_none() {
            self.meta = Some(tile.meta);
        }
        let bpp = tile.meta.format.bytes_per_pixel();
        let expected = tile.coord.width as usize * tile.coord.height as usize * bpp;
        if data.len() != expected {
            tracing::error!(
                "[webp-encoder] MISMATCH tile at ({},{}): data.len={} expected={} ({}x{} bpp={})",
                tile.coord.px,
                tile.coord.py,
                data.len(),
                expected,
                tile.coord.width,
                tile.coord.height,
                bpp,
            );
        }
        self.img_w = self.img_w.max(tile.coord.px + tile.coord.width);
        self.img_h = self.img_h.max(tile.coord.py + tile.coord.height);
        self.tiles.insert(
            (tile.coord.px, tile.coord.py),
            (tile.coord.width, tile.coord.height, data),
        );
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        self.done = true;
        if self.img_w == 0 || self.img_h == 0 {
            return Err(Error::internal("no image data for WebP export"));
        }
        let meta = self
            .meta
            .take()
            .ok_or_else(|| Error::internal("no pixel metadata received"))?;
        let bpp = meta.format.bytes_per_pixel();
        if bpp != 4 {
            return Err(Error::internal(format!(
                "WebPEncoderStage requires rgba8 tiles (bpp=4), got bpp={bpp}",
            )));
        }
        let iw = self.img_w as usize;
        let ih = self.img_h as usize;
        let row_stride = iw * bpp;
        let mut buffer = vec![0u8; ih * row_stride];
        for ((px, py), (tw, th, data)) in &self.tiles {
            let tile_row = *tw as usize * bpp;
            for row in 0..*th as usize {
                let src_start = row * tile_row;
                let src_end = src_start + tile_row;
                if src_end > data.len() {
                    break;
                }
                let dst_start = (*py as usize + row) * row_stride + *px as usize * bpp;
                let dst_end = dst_start + tile_row;
                if dst_end > buffer.len() {
                    break;
                }
                buffer[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
            }
        }
        let enc = Encoder::from_rgba(&buffer, self.img_w, self.img_h);
        let webp_data = enc.encode(self.quality);

        let file =
            File::create(&self.path).map_err(|e| Error::internal(format!("WebP create: {e}")))?;
        let mut w = BufWriter::new(file);
        std::io::Write::write_all(&mut w, &webp_data)
            .map_err(|e| Error::internal(format!("WebP write: {e}")))?;
        Ok(())
    }
}
