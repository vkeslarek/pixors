use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::ScanLine;
use crate::data::Buffer;
use crate::debug_stopwatch;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::color::ColorSpace;
use crate::model::pixel::meta::PixelMeta;
use crate::model::pixel::{AlphaPolicy, PixelFormat};
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};

static IS_INPUTS: &[PortDecl] = &[];
static IS_OUTPUTS: &[PortDecl] = &[PortDecl { name: "scanline", kind: DataKind::ScanLine }];
static IS_PORTS: PortSpec = PortSpec { inputs: IS_INPUTS, outputs: IS_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFileSource {
    pub path: PathBuf,
    pub layer_index: usize,
    pub layer_name: String,
}

impl Stage for ImageFileSource {
    fn kind(&self) -> &'static str { "image_file_source" }
    fn ports(&self) -> &'static PortSpec { &IS_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadOnly, prefers_gpu: false }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(ImageFileSourceRunner {
            path: self.path.clone(),
        }))
    }
}

pub struct ImageFileSourceRunner {
    path: PathBuf,
}

impl CpuKernel for ImageFileSourceRunner {
    fn process(&mut self, _item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("image_file_source");
        let file = File::open(&self.path)?;
        let decoder = png::Decoder::new(BufReader::new(file));
        let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let w = reader.info().width;
        let h = reader.info().height;
        let ct = reader.info().color_type;
        let meta = PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight);
        let size = reader.output_buffer_size().unwrap_or((w * h * 4) as usize);
        let mut buf = vec![0u8; size];
        reader.next_frame(&mut buf).map_err(|e| Error::Png(e.to_string()))?;
        let rgba = to_rgba8(&buf, w, h, ct);

        for y in 0..h {
            let s = y as usize * w as usize * 4;
            let data = rgba[s..s + w as usize * 4].to_vec();
            emit.emit(Item::ScanLine(ScanLine::new(y, w, meta, Buffer::cpu(data))));
        }
        Ok(())
    }
}

fn to_rgba8(data: &[u8], w: u32, h: u32, ct: png::ColorType) -> Vec<u8> {
    use png::ColorType;
    let pixels = (w * h) as usize;
    let mut rgba = vec![0u8; pixels * 4];
    match ct {
        ColorType::Rgba => rgba.copy_from_slice(data),
        ColorType::Rgb => {
            for i in 0..pixels {
                let s = i * 3;
                let d = i * 4;
                rgba[d..d + 3].copy_from_slice(&data[s..s + 3]);
                rgba[d + 3] = 255;
            }
        }
        ColorType::GrayscaleAlpha => {
            for i in 0..pixels {
                let s = i * 2;
                let d = i * 4;
                let g = data[s];
                rgba[d] = g; rgba[d + 1] = g; rgba[d + 2] = g;
                rgba[d + 3] = data[s + 1];
            }
        }
        ColorType::Grayscale => {
            for i in 0..pixels {
                let d = i * 4;
                let g = data[i];
                rgba[d] = g; rgba[d + 1] = g; rgba[d + 2] = g;
                rgba[d + 3] = 255;
            }
        }
        _ => { rgba.copy_from_slice(data); }
    }
    rgba
}
