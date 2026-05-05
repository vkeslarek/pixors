use iced::wgpu;

pub struct TiledTexture {
    texture: wgpu::Texture,
    full_view: wgpu::TextureView,
    sampler_linear: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,
    width: u32,
    height: u32,
    mip_level: u32,
    tile_size: u32,
}

impl TiledTexture {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        tile_size: u32,
        mip_level: u32,
    ) -> Self {
        let texture = Self::create_texture(device, width, height);
        fill_background(queue, &texture, width, height);
        let full_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler_linear = make_sampler(device, wgpu::FilterMode::Linear);
        let sampler_nearest = make_sampler(device, wgpu::FilterMode::Nearest);
        Self {
            texture,
            full_view,
            sampler_linear,
            sampler_nearest,
            width,
            height,
            mip_level,
            tile_size,
        }
    }

    fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport_tiled_texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        new_width: u32,
        new_height: u32,
        new_mip: u32,
    ) {
        if self.width == new_width && self.height == new_height && self.mip_level == new_mip {
            return;
        }
        let texture = Self::create_texture(device, new_width, new_height);
        fill_background(queue, &texture, new_width, new_height);
        let full_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler_linear = make_sampler(device, wgpu::FilterMode::Linear);
        let sampler_nearest = make_sampler(device, wgpu::FilterMode::Nearest);
        self.texture = texture;
        self.full_view = full_view;
        self.sampler_linear = sampler_linear;
        self.sampler_nearest = sampler_nearest;
        self.width = new_width;
        self.height = new_height;
        self.mip_level = new_mip;
    }

    pub fn write_tile_cpu(
        &self,
        queue: &wgpu::Queue,
        px: u32,
        py: u32,
        tile_w: u32,
        tile_h: u32,
        bytes: &[u8],
    ) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: px,
                    y: py,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tile_w * 4),
                rows_per_image: Some(tile_h),
            },
            wgpu::Extent3d {
                width: tile_w,
                height: tile_h,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
    pub fn view(&self) -> &wgpu::TextureView {
        &self.full_view
    }
    pub fn sampler(&self, linear: bool) -> &wgpu::Sampler {
        if linear {
            &self.sampler_linear
        } else {
            &self.sampler_nearest
        }
    }
    pub fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn mip_level(&self) -> u32 {
        self.mip_level
    }
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }
}

fn make_sampler(device: &wgpu::Device, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    })
}

/// Fill a texture with the viewport background colour so unwritten border tile
/// regions blend seamlessly with out-of-bounds areas while the pipeline runs.
/// Value is sRGB-encoded to match Rgba8UnormSrgb storage:
///   linear (0.067, 0.067, 0.075, 1.0) → sRGB ≈ (71, 71, 75, 255)
fn fill_background(queue: &wgpu::Queue, texture: &wgpu::Texture, width: u32, height: u32) {
    const FILL: [u8; 4] = [71, 71, 75, 255];
    let bpr = width * 4;
    let mut data = vec![0u8; (bpr * height) as usize];
    for row in 0..height as usize {
        for col in 0..width as usize {
            let off = row * bpr as usize + col * 4;
            data[off..off + 4].copy_from_slice(&FILL);
        }
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bpr),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}
