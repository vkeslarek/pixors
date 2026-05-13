use std::collections::HashMap;
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
    name: "tile",
    kind: DataKind::Tile,
}];
static JE_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(JE_INPUTS),
};

#[derive(Debug)]
pub struct JpegEncoderStage {
    path: PathBuf,
    quality: u8,
    meta: Option<PixelMeta>,
    tiles: HashMap<(u32, u32), (u32, u32, Vec<u8>)>,
    img_w: u32,
    img_h: u32,
    done: bool,
}

impl JpegEncoderStage {
    pub fn new(path: PathBuf, config: &EncoderConfig) -> Self {
        match config {
            EncoderConfig::Jpeg(cfg) => Self {
                path,
                quality: cfg.quality.clamp(1, 100),
                meta: None,
                tiles: HashMap::new(),
                img_w: 0,
                img_h: 0,
                done: false,
            },
            _ => panic!("wrong config type for JpegEncoderStage"),
        }
    }
}

impl Consumer for JpegEncoderStage {
    fn kind(&self) -> &'static str {
        "jpeg_encoder"
    }
    fn in_ports(&self) -> &'static InPortSpecification {
        &JE_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data: Vec<u8> = match &tile.data {
            Buffer::Cpu(v) => v.as_slice().to_vec(),
            Buffer::Gpu(_) => return Err(Error::internal("JpegEncoderStage requires CPU tiles")),
        };
        if self.meta.is_none() {
            self.meta = Some(tile.meta);
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
            return Err(Error::internal("no image data for JPEG export"));
        }
        let meta = self
            .meta
            .take()
            .ok_or_else(|| Error::internal("no pixel metadata received"))?;
        let bpp = meta.format.bytes_per_pixel();
        if bpp != 4 {
            return Err(Error::internal(format!(
                "JpegEncoderStage requires rgba8 tiles (bpp=4), got bpp={bpp}",
            )));
        }
        let iw = self.img_w as usize;
        let ih = self.img_h as usize;
        let dst_stride = iw * 3;
        let mut rgb = vec![0u8; ih * dst_stride];
        for ((px, py), (tw, th, data)) in &self.tiles {
            let src_stride = *tw as usize * bpp;
            for row in 0..*th as usize {
                let src_row = row * src_stride;
                if src_row + src_stride > data.len() {
                    break;
                }
                let dst_row = (*py as usize + row) * dst_stride + *px as usize * 3;
                for col in 0..*tw as usize {
                    let si = src_row + col * bpp;
                    let di = dst_row + col * 3;
                    if di + 2 >= rgb.len() || si + 2 >= data.len() {
                        break;
                    }
                    rgb[di] = data[si];
                    rgb[di + 1] = data[si + 1];
                    rgb[di + 2] = data[si + 2];
                }
            }
        }

        let mut comp = Compress::new(JpegColorSpace::JCS_RGB);
        comp.set_size(self.img_w as usize, self.img_h as usize);
        comp.set_quality(self.quality as f32);
        let mut started = comp
            .start_compress(Cursor::new(Vec::new()))
            .map_err(|e| Error::internal(format!("JPEG start: {e}")))?;

        for y in 0..ih {
            let start = y * dst_stride;
            let end = start + dst_stride;
            started
                .write_scanlines(&rgb[start..end])
                .map_err(|e| Error::internal(format!("JPEG write scanline: {e}")))?;
        }
        let jpeg_data = started
            .finish()
            .map_err(|e| Error::internal(format!("JPEG finish: {e}")))?
            .into_inner();

        let file =
            File::create(&self.path).map_err(|e| Error::internal(format!("JPEG create: {e}")))?;
        let mut w = BufWriter::new(file);
        std::io::Write::write_all(&mut w, &jpeg_data)
            .map_err(|e| Error::internal(format!("JPEG write: {e}")))?;
        Ok(())
    }
}
