use std::sync::Arc;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

pub struct TileAtlas {
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub width: u32,
    pub height: u32,
}

impl TileAtlas {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, width: u32, height: u32) -> Self {
        let tex_w = width.max(1);
        let tex_h = height.max(1);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile_atlas"),
            size: wgpu::Extent3d { width: tex_w, height: tex_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tile_atlas_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atlas_quad"),
            size: (std::mem::size_of::<Vertex>() * 6) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self { texture, bind_group, vertex_buffer, vertex_count: 0, width: tex_w, height: tex_h }
    }

    pub fn upload_tile(&self, queue: &wgpu::Queue, px: u32, py: u32, w: u32, h: u32, rgba8: &[u8]) {
        if w == 0 || h == 0 {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: px, y: py, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba8,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    pub fn set_full_quad(&mut self, queue: &wgpu::Queue, img_w: u32, img_h: u32) {
        let u2 = img_w as f32 / self.width as f32;
        let v2 = img_h as f32 / self.height as f32;
        let iw = img_w as f32;
        let ih = img_h as f32;

        let vertices: [Vertex; 6] = [
            Vertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
            Vertex { position: [iw, 0.0], uv: [u2, 0.0] },
            Vertex { position: [0.0, ih], uv: [0.0, v2] },
            Vertex { position: [0.0, ih], uv: [0.0, v2] },
            Vertex { position: [iw, 0.0], uv: [u2, 0.0] },
            Vertex { position: [iw, ih], uv: [u2, v2] },
        ];
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::bytes_of(&vertices));
        self.vertex_count = 6;
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_count == 0 {
            return;
        }
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_bind_group(1, &self.bind_group, &[]);
        rpass.draw(0..self.vertex_count, 0..1);
    }
}
