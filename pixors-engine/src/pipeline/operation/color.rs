use crate::color::ColorSpace;
use crate::convert::ColorConversion;
use crate::image::Tile;
use crate::image::buffer::BufferDesc;
use crate::pixel::{AlphaPolicy, Rgba};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use crate::stream::{Frame, FrameKind, Pipe};
use half::f16;
use std::borrow::Cow;
use std::sync::mpsc;

/// Transforms tile pixel data from source color space to destination.
#[derive(Clone)]
pub struct ColorConvertOperation {
    conv: ColorConversion,
    alpha: AlphaPolicy,
    output_f16: bool,
    src_desc: BufferDesc,
}

impl ColorConvertOperation {
    pub fn new(src: ColorSpace, dst: ColorSpace, alpha: AlphaPolicy, output_f16: bool, src_desc: BufferDesc) -> Result<Self, crate::error::Error> {
        Ok(Self { conv: src.converter_to(dst)?, alpha, output_f16, src_desc })
    }

    /// Simplified constructor for the new Tile-based pipeline.
    pub fn with_conv(conv: ColorConversion, alpha: AlphaPolicy) -> Self {
        Self { conv, alpha, output_f16: true, src_desc: BufferDesc::rgba8_interleaved(1, 1, ColorSpace::SRGB, crate::image::AlphaMode::Straight) }
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

impl Pipe for ColorConvertOperation {
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

// ── Operation (new API, Tile-based) ───────────────────────────────────────

impl Operation for ColorConvertOperation {
    type In = Tile<Rgba<f16>>;
    type Out = Tile<Rgba<f16>>;

    fn name(&self) -> &'static str { "color_convert" }

    fn process(&mut self, tile: Self::In, emit: &mut Emitter<Self::Out>) -> Result<(), crate::error::Error> {
        let converted: Vec<Rgba<f16>> = self.conv.convert_pixels(&tile.data, self.alpha);
        emit.emit(Tile::new(tile.coord, converted));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::TileCoord;
    use crate::pipeline::emitter::Emitter;

    fn make_tile(r: f32, g: f32, b: f32, a: f32, w: u32, h: u32) -> Tile<Rgba<f16>> {
        let coord = TileCoord::from_xywh(0, 0, 0, w, h);
        let pixels = vec![Rgba {
            r: f16::from_f32(r),
            g: f16::from_f32(g),
            b: f16::from_f32(b),
            a: f16::from_f32(a),
        }; (w * h) as usize];
        Tile::new(coord, pixels)
    }

    #[test]
    fn color_convert_op_srgb_to_acescg_opaque() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let mut op = ColorConvertOperation::with_conv(conv, AlphaPolicy::PremultiplyOnPack);

        let tile = make_tile(1.0, 0.0, 0.0, 1.0, 1, 1);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut emit = Emitter::new(tx);

        crate::pipeline::operation::Operation::process(&mut op, tile, &mut emit).unwrap();
        drop(emit);

        let result = rx.recv().unwrap();
        assert_eq!(result.data.len(), 1);
        let px = &result.data[0];
        assert!(px.r.to_f32() > 0.0, "red should be non-zero: {}", px.r.to_f32());
        assert!(px.g.to_f32() < 0.3, "green should be low for pure red");
        assert!(px.b.to_f32() < 0.1, "blue should be near zero for pure red");
        assert!((px.a.to_f32() - 1.0).abs() < 0.01, "alpha should be 1.0: {}", px.a.to_f32());
    }

    #[test]
    fn color_convert_op_preserves_alpha() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let mut op = ColorConvertOperation::with_conv(conv, AlphaPolicy::PremultiplyOnPack);

        let tile = make_tile(1.0, 1.0, 1.0, 0.5, 1, 1);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut emit = Emitter::new(tx);

        crate::pipeline::operation::Operation::process(&mut op, tile, &mut emit).unwrap();
        drop(emit);

        let result = rx.recv().unwrap();
        let px = &result.data[0];
        assert!((px.a.to_f32() - 0.5).abs() < 0.01, "alpha should be 0.5: {}", px.a.to_f32());
    }

    #[test]
    fn color_convert_op_roundtrip_acescg_and_back() {
        let to_acescg = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let to_srgb = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let mut op1 = ColorConvertOperation::with_conv(to_acescg, AlphaPolicy::PremultiplyOnPack);
        let mut op2 = ColorConvertOperation::with_conv(to_srgb, AlphaPolicy::Straight);

        let tile = make_tile(0.5, 0.3, 0.2, 1.0, 2, 2);

        let (tx1, rx1) = std::sync::mpsc::sync_channel(1);
        let mut emit1 = Emitter::new(tx1);
        crate::pipeline::operation::Operation::process(&mut op1, tile, &mut emit1).unwrap();
        drop(emit1);

        let intermediate = rx1.recv().unwrap();

        let (tx2, rx2) = std::sync::mpsc::sync_channel(1);
        let mut emit2 = Emitter::new(tx2);
        crate::pipeline::operation::Operation::process(&mut op2, intermediate, &mut emit2).unwrap();
        drop(emit2);

        let final_tile = rx2.recv().unwrap();
        assert_eq!(final_tile.data.len(), 4);

        for px in final_tile.data.iter() {
            let r = px.r.to_f32();
            let g = px.g.to_f32();
            let b = px.b.to_f32();
            assert!(r > 0.0 && g > 0.0 && b > 0.0,
                "pixel should be non-black after roundtrip: ({r:.3}, {g:.3}, {b:.3})");
            assert!((px.a.to_f32() - 1.0).abs() < 0.01, "alpha should be 1.0");
        }
    }

    #[test]
    fn color_convert_op_multiple_pixels() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::SRGB).unwrap();
        let mut op = ColorConvertOperation::with_conv(conv, AlphaPolicy::Straight);

        let tile = make_tile(0.8, 0.4, 0.1, 1.0, 16, 16);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut emit = Emitter::new(tx);

        crate::pipeline::operation::Operation::process(&mut op, tile, &mut emit).unwrap();
        drop(emit);

        let result = rx.recv().unwrap();
        assert_eq!(result.data.len(), 256);
        for px in result.data.iter() {
            let r = px.r.to_f32();
            let g = px.g.to_f32();
            let b = px.b.to_f32();
            assert!((r - 0.8).abs() < 0.02, "red should be ~0.8: {r:.3}");
            assert!((g - 0.4).abs() < 0.02, "green should be ~0.4: {g:.3}");
            assert!((b - 0.1).abs() < 0.02, "blue should be ~0.1: {b:.3}");
        }
    }
}
