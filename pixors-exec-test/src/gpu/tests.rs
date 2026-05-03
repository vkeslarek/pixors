#[cfg(test)]
mod tests {
    use crate::container::meta::PixelMeta;
    use crate::container::{EdgeCondition, Neighborhood, Tile, TileCoord};
    use crate::pipeline::exec_graph::emitter::Emitter;
    use crate::pipeline::exec_graph::item::Item;
    use crate::pipeline::exec_graph::runner::OperationRunner;
    use crate::pipeline::exec::{
        blur_kernel, download, upload,
    };
    use crate::pixel::{AlphaPolicy, PixelFormat};
    use crate::gpu::Buffer;

    /// GPU smoke test: blur a 32×32 RGBA8 tile via Upload → BlurKernelGpu →
    /// Download and check the result against the CPU kernel within ±1 per
    /// channel (sliding-window vs naive box average rounding can differ).
    /// Ignored on CI hosts without a GPU adapter.
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
        let meta = PixelMeta::new(PixelFormat::Rgba8, crate::color::ColorSpace::SRGB, AlphaPolicy::Straight);
        let coord = TileCoord::new(0, 0, w, w, h);

        // CPU reference.
        let cpu_tile = Tile::new(coord, meta, Buffer::cpu(data.clone()));
        let cpu_nbhd = Neighborhood::new(r, coord, vec![cpu_tile], EdgeCondition::Clamp, meta, w, h, w);
        let mut cpu_runner = blur_kernel::BlurKernelRunner::new(r);
        let mut cpu_emit = Emitter::new();
        cpu_runner.process(Item::Neighborhood(cpu_nbhd), &mut cpu_emit).unwrap();
        let cpu_out = match cpu_emit.into_items().remove(0) {
            Item::Tile(t) => t,
            _ => panic!(),
        };
        let cpu_bytes: Vec<u8> = cpu_out.data.as_cpu_slice().unwrap().to_vec();

        // GPU path.
        let gpu_tile_cpu = Tile::new(coord, meta, Buffer::cpu(data.clone()));
        let mut up = upload::UploadRunner::new();
        let mut up_emit = Emitter::new();
        up.process(Item::Tile(gpu_tile_cpu), &mut up_emit).unwrap();
        let gpu_tile = match up_emit.into_items().remove(0) {
            Item::Tile(t) => t,
            _ => panic!(),
        };
        assert!(gpu_tile.data.is_gpu());

        let gpu_nbhd = Neighborhood::new(r, coord, vec![gpu_tile], EdgeCondition::Clamp, meta, w, h, w);
        let mut gpu_runner = blur_kernel::BlurKernelGpuRunner::new(r);
        let mut gpu_emit = Emitter::new();
        gpu_runner.process(Item::Neighborhood(gpu_nbhd), &mut gpu_emit).unwrap();
        let blurred_gpu = match gpu_emit.into_items().remove(0) {
            Item::Tile(t) => t,
            _ => panic!(),
        };

        let mut down = download::DownloadRunner::new();
        let mut down_emit = Emitter::new();
        down.process(Item::Tile(blurred_gpu), &mut down_emit).unwrap();
        let cpu_back = match down_emit.into_items().remove(0) {
            Item::Tile(t) => t,
            _ => panic!(),
        };
        let gpu_bytes: Vec<u8> = cpu_back.data.as_cpu_slice().unwrap().to_vec();

        assert_eq!(cpu_bytes.len(), gpu_bytes.len());
        let mut max_diff = 0i32;
        for (a, b) in cpu_bytes.iter().zip(gpu_bytes.iter()) {
            let d = (*a as i32 - *b as i32).abs();
            if d > max_diff {
                max_diff = d;
            }
        }
        assert!(max_diff <= 1, "max channel diff {} > 1", max_diff);
    }
}
