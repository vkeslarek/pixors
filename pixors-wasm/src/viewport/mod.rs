pub mod atlas;
pub mod camera;
pub mod ram_cache;
pub mod renderer;

use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;

use half::f16;
use crate::checkerboard::CheckerboardSource;
use crate::color::ColorSpace;
use crate::pipeline::job::Job;
use crate::pipeline::sink::Sink;
use crate::pixel::{AlphaPolicy, Rgba};

use atlas::TileAtlas;
use camera::Camera;
use ram_cache::RamTileCache;
use renderer::Renderer;

pub struct GpuViewport {
    pub camera: Camera,
    pub renderer: Renderer,
    pub atlas: TileAtlas,
    cache: Arc<Mutex<RamTileCache>>,
    tile_size: u32,
    needs_upload: bool,
}

impl GpuViewport {
    pub async fn new(vp_w: u32, vp_h: u32, img_w: u32, img_h: u32, tile_size: u32) -> Result<Self, String> {
        let renderer = Renderer::new(vp_w, vp_h).await?;

        let atlas = TileAtlas::new(
            &renderer.device,
            &renderer.atlas_bind_group_layout,
            img_w,
            img_h,
        );

        let camera = Camera::new(vp_w, vp_h, img_w, img_h);

        let mut vp = Self {
            camera,
            renderer,
            atlas,
            cache: Arc::new(Mutex::new(RamTileCache::new())),
            tile_size,
            needs_upload: true,
        };

        vp.load_checkerboard();
        vp.camera.fit();
        vp.renderer.update_camera(&vp.camera.to_uniform());
        vp.atlas.set_full_quad(&vp.renderer.queue, img_w, img_h);
        vp.upload_visible();

        Ok(vp)
    }

    fn load_checkerboard(&mut self) {
        let conv = ColorSpace::ACES_CG
            .converter_to(ColorSpace::SRGB)
            .expect("ACEScg to sRGB conversion");

        let source = CheckerboardSource {
            img_w: self.camera.img_w,
            img_h: self.camera.img_h,
            tile_size: self.tile_size,
        };

        let cache = Arc::clone(&self.cache);
        let sink = RamCacheSink { conv, cache };

        Job::from_source(source)
            .sink(sink)
            .join();
    }

    pub fn upload_visible(&mut self) {
        let cache = self.cache.lock().unwrap();
        let tiles = self.camera.visible_tiles(self.tile_size);
        for tile in &tiles {
            if let Some(data) = cache.get(0, tile.tx, tile.ty) {
                let flat: Vec<u8> = bytemuck::cast_slice::<Rgba<u8>, u8>(data).to_vec();
                self.atlas.upload_tile(
                    &self.renderer.queue,
                    tile.px,
                    tile.py,
                    tile.width,
                    tile.height,
                    &flat,
                );
            }
        }
        self.needs_upload = false;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.pan(dx, dy);
        self.renderer.update_camera(&self.camera.to_uniform());
        self.needs_upload = true;
    }

    pub fn zoom_at(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        self.camera.zoom_at(factor, anchor_x, anchor_y);
        self.renderer.update_camera(&self.camera.to_uniform());
        self.needs_upload = true;
    }

    pub fn fit(&mut self) {
        self.camera.fit();
        self.renderer.update_camera(&self.camera.to_uniform());
        self.needs_upload = true;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.queue_resize(width, height);
        self.camera.resize(width, height);
    }

    pub async fn render(&self) -> Result<js_sys::Uint8Array, JsValue> {
        let map = {
            let atlas_bg = &self.atlas.bind_group;
            let atlas_vb = &self.atlas.vertex_buffer;
            let atlas_vc = self.atlas.vertex_count;
            self.renderer.submit(|rpass| {
                if atlas_vc == 0 { return; }
                rpass.set_vertex_buffer(0, atlas_vb.slice(..));
                rpass.set_bind_group(1, atlas_bg, &[]);
                rpass.draw(0..atlas_vc, 0..1);
            })
        };
        map.await.map_err(|e| JsValue::from_str(&format!("map: {:?}", e)))?;
        self.renderer.read_pixels()
    }

    pub fn prepare_frame(&mut self) {
        if self.needs_upload {
            self.upload_visible();
        }
    }
}

struct RamCacheSink {
    conv: crate::color::ColorConversion,
    cache: Arc<Mutex<RamTileCache>>,
}

impl Sink for RamCacheSink {
    type Item = crate::image::Tile<Rgba<half::f16>>;

    fn consume(&self, item: Self::Item) -> Result<(), crate::error::Error> {
        let rgba8: Vec<Rgba<u8>> = self.conv.convert_pixels::<Rgba<half::f16>, Rgba<u8>>(
            &item.data,
            AlphaPolicy::Straight,
        );
        self.cache.lock().unwrap().put(0, item.coord.tx, item.coord.ty, rgba8);
        Ok(())
    }
}
