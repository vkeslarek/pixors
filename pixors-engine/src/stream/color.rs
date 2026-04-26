use crate::color::ColorSpace;
use crate::convert::ColorConversion;
use crate::image::buffer::BufferDesc;
use crate::pixel::{AlphaPolicy, Rgba};
use crate::stream::{Frame, FrameKind, Pipe};
use half::f16;
use std::borrow::Cow;
use std::sync::mpsc;

/// Transforms tile pixel data from source color space to destination.
pub struct ColorConvertPipe {
    conv: ColorConversion,
    alpha: AlphaPolicy,
    output_f16: bool,
    /// Source image descriptor (carries plane count: 1=gray, 3=RGB, 4=RGBA)
    src_desc: BufferDesc,
}

impl ColorConvertPipe {
    pub fn new(src: ColorSpace, dst: ColorSpace, alpha: AlphaPolicy, output_f16: bool, src_desc: BufferDesc) -> Result<Self, crate::error::Error> {
        Ok(Self { conv: src.converter_to(dst)?, alpha, output_f16, src_desc })
    }
}

impl Pipe for ColorConvertPipe {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame> {
        let (tx, out) = mpsc::sync_channel(64);
        let alpha = self.alpha;
        let output_f16 = self.output_f16;
        let src_desc = self.src_desc;
        std::thread::spawn(move || {
            while let Ok(mut frame) = rx.recv() {
                let is_terminal = frame.is_terminal();

                if let FrameKind::Tile { coord } = frame.kind {
                    let src = frame.data.as_ref();
                    let bpp = src_desc.planes.len() * src_desc.planes[0].encoding.byte_size();
                    let tile_n = (coord.width * coord.height) as usize;
                    if tile_n == 0 { if tx.send(frame).is_err() { return; }; continue; }
                    let actual_len = (tile_n * bpp).min(src.len());
                    if actual_len == 0 { if tx.send(frame).is_err() { return; }; continue; }

                    // Build tile descriptor matching source layout (RGB=3 planes, RGBA=4, etc.)
                    let mut desc = src_desc.clone();
                    desc.width = coord.width;
                    desc.height = coord.height;
                    let tile_stride = coord.width as usize * bpp;
                    for p in &mut desc.planes {
                        p.row_length = coord.width;
                        p.row_stride = tile_stride;
                    }

                    if output_f16 {
                        let converted: Vec<Rgba<f16>> = self.conv.convert_buffer(&src[..actual_len], &desc, alpha);
                        frame.data = Cow::Owned(bytemuck::cast_slice::<Rgba<f16>, u8>(&converted).to_vec());
                        frame.meta.color_space = ColorSpace::ACES_CG;
                    } else {
                        let converted: Vec<[u8;4]> = self.conv.convert_buffer(&src[..actual_len], &desc, alpha);
                        frame.data = Cow::Owned(bytemuck::cast_slice::<[u8;4], u8>(&converted).to_vec());
                        frame.meta.color_space = self.conv.dst();
                    }
                }

                if tx.send(frame).is_err() { return; }
                if is_terminal { break; }
            }
        });
        out
    }
}
