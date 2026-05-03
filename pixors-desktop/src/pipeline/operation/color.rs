use std::sync::Arc;
use pixors_engine::color::ColorConversion;
use pixors_engine::image::Tile;
use pixors_engine::pixel::{AlphaPolicy, Rgba};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use half::f16;

/// Transforms tile pixel data from source color space to destination.
#[derive(Clone)]
pub struct ColorConvertOperation {
    conv: ColorConversion,
    alpha: AlphaPolicy,
}

impl ColorConvertOperation {
    pub fn with_conv(conv: ColorConversion, alpha: AlphaPolicy) -> Self {
        Self { conv, alpha }
    }
}

// ── Operation (Tile-based API) ───────────────────────────────────────

impl Operation for ColorConvertOperation {
    type In = Tile<Rgba<f16>>;
    type Out = Tile<Rgba<f16>>;

    fn name(&self) -> &'static str { "color_convert" }

    fn process(&mut self, tile: Arc<Self::In>, emit: &mut Emitter<Self::Out>) -> Result<(), pixors_engine::error::Error> {
        let converted: Vec<Rgba<f16>> = self.conv.convert_pixels(&tile.data, self.alpha);
        emit.emit(Tile::new(tile.coord, converted));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixors_engine::color::ColorSpace;
    use pixors_engine::image::TileCoord;
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

        Operation::process(&mut op, Arc::new(tile), &mut emit).unwrap();
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

        crate::pipeline::operation::Operation::process(&mut op, Arc::new(tile), &mut emit).unwrap();
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
        crate::pipeline::operation::Operation::process(&mut op1, Arc::new(tile), &mut emit1).unwrap();
        drop(emit1);

        let intermediate = rx1.recv().unwrap();

        let (tx2, rx2) = std::sync::mpsc::sync_channel(1);
        let mut emit2 = Emitter::new(tx2);
        crate::pipeline::operation::Operation::process(&mut op2, Arc::new(intermediate), &mut emit2).unwrap();
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

        crate::pipeline::operation::Operation::process(&mut op, Arc::new(tile), &mut emit).unwrap();
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
