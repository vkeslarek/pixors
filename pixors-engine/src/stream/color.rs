use crate::color::ColorSpace;
use crate::convert::ColorConversion;
use crate::image::buffer::BufferDesc;
use crate::pixel::{AlphaPolicy, Rgba};
use crate::stream::{Frame, FrameKind, Pipe};
use half::f16;
use std::borrow::Cow;
use std::sync::mpsc;

/// Transforms tile pixel data from source color space to destination.
#[derive(Clone)]
pub struct ColorConvertPipe {
    conv: ColorConversion,
    alpha: AlphaPolicy,
    output_f16: bool,
    src_desc: BufferDesc,
}

impl ColorConvertPipe {
    pub fn new(src: ColorSpace, dst: ColorSpace, alpha: AlphaPolicy, output_f16: bool, src_desc: BufferDesc) -> Result<Self, crate::error::Error> {
        Ok(Self { conv: src.converter_to(dst)?, alpha, output_f16, src_desc })
    }

    /// Convert a single tile frame. Returns the frame unchanged for non-tile frames.
    pub fn process(&self, mut frame: Frame) -> Frame {
        if let FrameKind::Tile { coord } = frame.kind {
            let src = frame.data.as_ref();
            let bpp = self.src_desc.planes.len() * self.src_desc.planes[0].encoding.byte_size();
            let tile_n = (coord.width * coord.height) as usize;
            if tile_n == 0 { return frame; }
            let actual_len = (tile_n * bpp).min(src.len());
            if actual_len == 0 { return frame; }

            let mut desc = self.src_desc.clone();
            desc.width = coord.width;
            desc.height = coord.height;
            let tile_stride = coord.width as usize * bpp;
            for p in &mut desc.planes {
                p.row_length = coord.width;
                p.row_stride = tile_stride;
            }

            if self.output_f16 {
                let converted: Vec<Rgba<f16>> = self.conv.convert_buffer(&src[..actual_len], &desc, self.alpha);
                frame.data = Cow::Owned(bytemuck::cast_slice::<Rgba<f16>, u8>(&converted).to_vec());
                frame.meta.color_space = ColorSpace::ACES_CG;
            } else {
                let converted: Vec<[u8;4]> = self.conv.convert_buffer(&src[..actual_len], &desc, self.alpha);
                frame.data = Cow::Owned(bytemuck::cast_slice::<[u8;4], u8>(&converted).to_vec());
                frame.meta.color_space = self.conv.dst();
            }
        }
        frame
    }
}

impl Pipe for ColorConvertPipe {
    /// Single-threaded pipeline adapter. For parallel processing, use [`ParPipe`] wrapping `process`.
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame> {
        let (tx, out) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            while let Ok(mut frame) = rx.recv() {
                let is_terminal = frame.is_terminal();
                frame = self.process(frame);
                if tx.send(frame).is_err() { return; }
                if is_terminal { break; }
            }
        });
        out
    }
}
