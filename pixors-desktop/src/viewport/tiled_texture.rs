use iced::wgpu;

pub struct TiledTexture {
    texture: wgpu::Texture,
    full_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    tile_size: u32,
}

impl TiledTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, tile_size: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_tiled_texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let full_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self { texture, full_view, sampler, width, height, tile_size }
    }

    /// Upload one tile from a CPU byte slice. `bytes` is packed `tile_w * tile_h * 4` RGBA8.
    pub fn write_tile_cpu(
        &self,
        queue: &wgpu::Queue,
        px: u32,
        py: u32,
        tile_w: u32,
        tile_h: u32,
        bytes: &[u8],
    ) {
        let bpr = tile_w * 4;
        let aligned_bpr = bpr.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        // wgpu requires bytes_per_row to be aligned to COPY_BYTES_PER_ROW_ALIGNMENT (256).
        // For 256-wide tiles (bpr = 1024) alignment is free; edge tiles need padding.
        let data: std::borrow::Cow<[u8]> = if aligned_bpr == bpr {
            std::borrow::Cow::Borrowed(bytes)
        } else {
            let mut padded = vec![0u8; (aligned_bpr * tile_h) as usize];
            for y in 0..tile_h as usize {
                let row = bpr as usize;
                padded[y * aligned_bpr as usize..y * aligned_bpr as usize + row]
                    .copy_from_slice(&bytes[y * row..(y + 1) * row]);
            }
            std::borrow::Cow::Owned(padded)
        };

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: px, y: py, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(tile_h),
            },
            wgpu::Extent3d { width: tile_w, height: tile_h, depth_or_array_layers: 1 },
        );
    }

    pub fn view(&self) -> &wgpu::TextureView { &self.full_view }
    pub fn sampler(&self) -> &wgpu::Sampler { &self.sampler }
    pub fn dims(&self) -> (u32, u32) { (self.width, self.height) }
}
