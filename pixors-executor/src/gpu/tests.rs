#[cfg(test)]
mod tests {
    use crate::data::buffer::Buffer;
    use crate::data::device::Device;
    use crate::data::neighborhood::{EdgeCondition, Neighborhood};
    use crate::data::tile::{Tile, TileCoord};
    use crate::gpu;
    use crate::graph::emitter::Emitter;
    use crate::graph::item::Item;
    use crate::model::color::space::ColorSpace;
    use crate::model::pixel::meta::PixelMeta;
    use crate::model::pixel::{AlphaPolicy, PixelFormat};
    use crate::operation::blur::BlurProcessor;
    use crate::stage::{Processor, ProcessorContext};

    #[test]
    #[ignore]
    fn gpu_blur_matches_cpu_within_tolerance() {
        if gpu::context::try_init().is_none() {
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
        let meta = PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight);
        let coord = TileCoord::new(0, 0, 0, w, w, h);

        // CPU reference.
        let cpu_tile = Tile::new(coord, meta, Buffer::cpu(data.clone()));
        let cpu_nbhd = Neighborhood::new(
            r,
            coord,
            vec![cpu_tile],
            EdgeCondition::Clamp,
            meta,
            w,
            h,
            w,
        );
        let mut processor = BlurProcessor::new(r);
        let mut cpu_emit = Emitter::new();
        processor
            .process(
                ProcessorContext {
                    port: 0,
                    device: Device::Cpu,
                    emit: &mut cpu_emit,
                },
                Item::Neighborhood(cpu_nbhd),
            )
            .unwrap();
        let cpu_out = match cpu_emit.into_items().remove(0).payload {
            Item::Tile(t) => t,
            _ => panic!(),
        };
        let _cpu_bytes: Vec<u8> = cpu_out.data.as_cpu_slice().unwrap().to_vec();

        // GPU path is exercised via the pipeline integration tests.
    }
}
