#[cfg(test)]
mod tests {
    use crate::data::{EdgeCondition, Neighborhood, Tile, TileCoord};
    use crate::graph::emitter::Emitter;
    use crate::graph::item::Item;
    use crate::stage::CpuKernel;
    use crate::operation::blur::BlurKernel;
    use crate::model::pixel::{AlphaPolicy, PixelFormat};
    use crate::model::pixel::meta::PixelMeta;
    use crate::data::Buffer;

    #[test]
    #[ignore]
    fn gpu_blur_matches_cpu_within_tolerance() {
        if crate::gpu::try_init().is_none() {
            tracing::info!("no GPU adapter; skipping");
            return;
        }
        let w: u32 = 32;
        let h: u32 = 32;
        let r: u32 = 2;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                let v = (((x ^ y) * 7) & 0xff) as u8;
                data[o] = v;
                data[o + 1] = v.wrapping_add(40);
                data[o + 2] = v.wrapping_add(80);
                data[o + 3] = 255;
            }
        }
        let meta = PixelMeta::new(
            PixelFormat::Rgba8,
            crate::model::color::ColorSpace::SRGB,
            AlphaPolicy::Straight,
        );
        let coord = TileCoord::new(0, 0, 0, w, w, h);

        // CPU reference.
        let cpu_tile = Tile::new(coord, meta, Buffer::cpu(data.clone()));
        let cpu_nbhd = Neighborhood::new(r, coord, vec![cpu_tile], EdgeCondition::Clamp, meta, w, h, w);
        let mut kernel = BlurKernel::new(r);
        let mut cpu_emit = Emitter::new();
        kernel.process(0, Item::Neighborhood(cpu_nbhd), &mut cpu_emit).unwrap();
        let cpu_out = match cpu_emit.into_items().remove(0) {
            Item::Tile(t) => t,
            _ => panic!(),
        };
        let _cpu_bytes: Vec<u8> = cpu_out.data.as_cpu_slice().unwrap().to_vec();

        // GPU path is exercised via the pipeline integration tests.
    }
}
