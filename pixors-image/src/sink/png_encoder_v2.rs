use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::codec::{EncoderConfig, EncoderDescriptor, ImageEncoder, PngExportConfig};
use crate::image::Dpi;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    Consumer, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage,
};

static PGV2_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static PGV2_OUTPUTS: &[PortDeclaration] = &[];
static PGV2_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(PGV2_INPUTS),
    outputs: PortGroup::Fixed(PGV2_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngEncoderV2 {
    pub path: PathBuf,
    pub config: PngExportConfig,
    pub dpi: Option<Dpi>,
    pub icc_profile: Option<Vec<u8>>,
}

impl Stage for PngEncoderV2 {
    fn kind(&self) -> &'static str {
        "png_encoder_v2"
    }
    fn ports(&self) -> &'static PortSpecification {
        &PGV2_PORTS
    }
    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        Some(Box::new(PngEncoderConsumer::new(
            self.path.clone(),
            self.config.clone(),
            self.dpi,
            self.icc_profile.clone(),
        )))
    }
}

struct PngEncoderConsumer {
    path: PathBuf,
    config: PngExportConfig,
    dpi: Option<Dpi>,
    icc_profile: Option<Vec<u8>>,
    // (px, py) → (width, height, data)
    tiles: HashMap<(u32, u32), (u32, u32, Vec<u8>)>,
    image_width: u32,
    image_height: u32,
    meta: Option<PixelMeta>,
}

impl PngEncoderConsumer {
    fn new(
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

impl Consumer for PngEncoderConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;

        let data: Vec<u8> = match tile.data {
            Buffer::Cpu(v) => match Arc::try_unwrap(v) {
                Ok(owned) => owned,
                Err(shared) => (*shared).clone(),
            },
            Buffer::Gpu(_) => return Err(Error::internal("PngEncoderV2 requires CPU tiles")),
        };

        if self.meta.is_none() {
            self.meta = Some(tile.meta);
        }

        self.image_width = self.image_width.max(tile.coord.px + tile.coord.width);
        self.image_height = self.image_height.max(tile.coord.py + tile.coord.height);

        self.tiles.insert(
            (tile.coord.px, tile.coord.py),
            (tile.coord.width, tile.coord.height, data),
        );
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
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

        let encoder = crate::png::PngEncoder;
        encoder.encode(
            &self.path,
            &buffer,
            &desc,
            &EncoderConfig::Png(self.config.clone()),
        )
    }
}
