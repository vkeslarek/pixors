use std::collections::HashSet;

/// Manages a tiled GPU texture for the viewport.
/// MIP chain is deferred (mip_level_count = 1 until storage texture support is available).
pub struct TiledTexture {
    pub texture: iced::wgpu::Texture,
    pub full_view: iced::wgpu::TextureView,
    pub sampler: iced::wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub dirty_tiles: HashSet<(u32, u32)>,
}

impl TiledTexture {
    pub fn new(device: &iced::wgpu::Device, width: u32, height: u32, tile_size: u32) -> Self {
        let texture = device.create_texture(&iced::wgpu::TextureDescriptor {
            label: Some("viewport_tiled_texture"),
            size: iced::wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: iced::wgpu::TextureDimension::D2,
            format: iced::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: iced::wgpu::TextureUsages::TEXTURE_BINDING
                | iced::wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let full_view = texture.create_view(&iced::wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&iced::wgpu::SamplerDescriptor {
            address_mode_u: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_v: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_w: iced::wgpu::AddressMode::ClampToEdge,
            mag_filter: iced::wgpu::FilterMode::Linear,
            min_filter: iced::wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture,
            full_view,
            sampler,
            width,
            height,
            tile_size,
            dirty_tiles: HashSet::new(),
        }
    }

    pub fn write_tile_cpu(
        &mut self,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        px: u32,
        py: u32,
        tile_w: u32,
        tile_h: u32,
        bytes: &[u8],
    ) {
        let pad = (256 - (tile_w * 4) % 256) % 256;
        let row_pitch = tile_w * 4 + pad;
        let padded_len = (row_pitch * tile_h) as usize;
        let mut padded = vec![0u8; padded_len];

        for y in 0..tile_h as usize {
            let src = y * tile_w as usize * 4;
            let dst = y * row_pitch as usize;
            let len = (tile_w as usize * 4).min(bytes.len().saturating_sub(src));
            padded[dst..dst + len].copy_from_slice(&bytes[src..src + len]);
        }

        let staging = device.create_buffer(&iced::wgpu::BufferDescriptor {
            label: Some("tile_staging"),
            size: padded_len as u64,
            usage: iced::wgpu::BufferUsages::COPY_SRC | iced::wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: true,
        });
        staging.slice(..).get_mapped_range_mut()[..padded_len].copy_from_slice(&padded);
        staging.unmap();

        let mut encoder =
            device.create_command_encoder(&iced::wgpu::CommandEncoderDescriptor {
                label: Some("tile_upload"),
            });

        encoder.copy_buffer_to_texture(
            iced::wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: iced::wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_pitch),
                    rows_per_image: Some(tile_h),
                },
            },
            iced::wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: iced::wgpu::Origin3d {
                    x: px,
                    y: py,
                    z: 0,
                },
                aspect: iced::wgpu::TextureAspect::All,
            },
            iced::wgpu::Extent3d {
                width: tile_w,
                height: tile_h,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let tx = px / self.tile_size;
        let ty = py / self.tile_size;
        self.dirty_tiles.insert((tx, ty));
    }

    pub fn view(&self) -> &iced::wgpu::TextureView {
        &self.full_view
    }

    pub fn sampler(&self) -> &iced::wgpu::Sampler {
        &self.sampler
    }

    pub fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
