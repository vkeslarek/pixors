use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::color::ColorSpace;
use crate::container::ScanLine;
use crate::container::meta::PixelMeta;
use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::SourceRunner;
use super::{Device, Stage, StageRole};
use crate::error::Error;
use crate::pixel::{AlphaPolicy, PixelFormat};
use crate::gpu::Buffer;
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDecoder {
    pub path: PathBuf,
}

impl Stage for FileDecoder {
    fn kind(&self) -> &'static str { "file_decoder" }
    fn device(&self) -> Device { Device::Cpu }
    fn allocates_output(&self) -> bool { true }
    fn role(&self) -> StageRole { StageRole::Source }
    fn source_runner(&self) -> Result<Box<dyn SourceRunner>, Error> {
        Ok(Box::new(FileDecoderRunner::new(self.path.clone())))
    }
}

pub struct FileDecoderRunner {
    path: PathBuf,
}

impl FileDecoderRunner {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SourceRunner for FileDecoderRunner {
    fn run(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("file_decoder");
        let file = File::open(&self.path)?;
        let mut decoder = png::Decoder::new(BufReader::new(file));
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let w = reader.info().width;
        let h = reader.info().height;
        let ct = reader.info().color_type;
        let meta = PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight);
        let size = reader.output_buffer_size().unwrap_or((w * h * 4) as usize);
        let mut buf = vec![0u8; size];
        reader
            .next_frame(&mut buf)
            .map_err(|e| Error::Png(e.to_string()))?;
        let rgba = to_rgba8(&buf, w, h, ct);

        for y in 0..h {
            let s = y as usize * w as usize * 4;
            let data = rgba[s..s + w as usize * 4].to_vec();
            emit.emit(Item::ScanLine(ScanLine::new(y, w, meta, Buffer::cpu(data))));
        }
        Ok(())
    }
}

fn to_rgba8(raw: &[u8], w: u32, h: u32, ct: png::ColorType) -> Vec<u8> {
    match ct {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => {
            let mut out = vec![0u8; w as usize * h as usize * 4];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let si = (y * w as usize + x) * 3;
                    let di = (y * w as usize + x) * 4;
                    out[di..di + 3].copy_from_slice(&raw[si..si + 3]);
                    out[di + 3] = 255;
                }
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = vec![0u8; w as usize * h as usize * 4];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let si = y * w as usize + x;
                    let di = (y * w as usize + x) * 4;
                    let v = raw[si];
                    out[di] = v;
                    out[di + 1] = v;
                    out[di + 2] = v;
                    out[di + 3] = 255;
                }
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = vec![0u8; w as usize * h as usize * 4];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let si = (y * w as usize + x) * 2;
                    let di = (y * w as usize + x) * 4;
                    let v = raw[si];
                    out[di] = v;
                    out[di + 1] = v;
                    out[di + 2] = v;
                    out[di + 3] = raw[si + 1];
                }
            }
            out
        }
        _ => raw.to_vec(),
    }
}
