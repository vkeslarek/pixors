use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::codec::{EncoderConfig, EncoderDescriptor, ImageEncoder, PngExportConfig};
use crate::image::Dpi;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

static PGV2_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static PGV2_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(PGV2_INPUTS),
};

#[derive(Debug, Clone)]
pub struct PngEncoderV2 {
    pub path: PathBuf,
    pub config: PngExportConfig,
    pub dpi: Option<Dpi>,
    pub icc_profile: Option<Vec<u8>>,
    tiles: HashMap<(u32, u32), (u32, u32, Vec<u8>)>,
    image_width: u32,
    image_height: u32,
    meta: Option<PixelMeta>,
}

impl PngEncoderV2 {
    pub fn new(
        path: PathBuf,
        config: PngExportConfig,
        dpi: Option<Dpi>,
        icc_profile: Option<Vec<u8>>,
    ) -> Self {
        Self {
            path,
            config,
            dpi,
            icc_profile,
            tiles: HashMap::new(),
            image_width: 0,
            image_height: 0,
            meta: None,
        }
    }
}

impl Consumer for PngEncoderV2 {
    fn kind(&self) -> &'static str {
        "png_encoder_v2"
    }
    fn in_ports(&self) -> &'static InPortSpecification {
        &PGV2_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data: Vec<u8> = match tile.data {
            Buffer::Cpu(v) => match Arc::try_unwrap(v) {
                Ok(owned) => owned,
                Err(shared) => (*shared).clone(),
            },
            Buffer::Gpu(_) => return Err(Error::internal("PngEncoderV2 requires CPU tiles")),
        };
        if self.meta.is_none() { self.meta = Some(tile.meta); }
        self.image_width = self.image_width.max(tile.coord.px + tile.coord.width);
        self.image_height = self.image_height.max(tile.coord.py + tile.coord.height);
        let expected = tile.coord.width as usize * tile.coord.height as usize * tile.meta.format.bytes_per_pixel();
        let data_len = data.len();
        if data_len != expected {
            tracing::warn!(
                "[encoder] PNG tile size mismatch at ({},{}): data.len={} expected={expected} ({}x{} bpp={})",
                tile.coord.px, tile.coord.py, data_len, tile.coord.width, tile.coord.height, tile.meta.format.bytes_per_pixel(),
            );
        }
        self.tiles.insert(
            (tile.coord.px, tile.coord.py),
            (tile.coord.width, tile.coord.height, data),
        );
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        const MAX_PIXELS: usize = 128 * 1024 * 1024; // ~512 MB at RGBA8
        let iw = self.image_width as usize;
        let ih = self.image_height as usize;
        if iw * ih > MAX_PIXELS {
            return Err(Error::internal(format!(
                "PNG export too large: {}x{} = {} pixels (max {})",
                iw,
                ih,
                iw * ih,
                MAX_PIXELS
            )));
        }
        let meta = self
            .meta
            .take()
            .ok_or_else(|| Error::internal("no pixel metadata received"))?;
        let bpp = meta.format.bytes_per_pixel();
        let iw = self.image_width as usize;
        let ih = self.image_height as usize;
        let mut buffer = vec![0u8; iw * ih * bpp];
        for ((px, py), (tw, th, data)) in &self.tiles {
            let row_bytes = *tw as usize * bpp;
            for row in 0..*th as usize {
                let src_start = row * row_bytes;
                let src_end = src_start + row_bytes;
                if src_end > data.len() {
                    break;
                }
                let dst_start = (*py as usize + row) * iw * bpp + *px as usize * bpp;
                let dst_end = dst_start + row_bytes;
                if dst_end > buffer.len() {
                    break;
                }
                buffer[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
            }
        }
        let desc = EncoderDescriptor {
            width: self.image_width,
            height: self.image_height,
            pixel_format: meta.format,
            color_space: meta.color_space,
            alpha_policy: meta.alpha_policy,
            dpi: self.dpi,
            icc_profile: self.icc_profile.clone(),
            metadata: Vec::new(),
        };
        crate::png::PngEncoder.encode(
            &self.path,
            &buffer,
            &desc,
            &EncoderConfig::Png(self.config.clone()),
        )
    }
}
