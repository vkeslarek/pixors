use crate::image::{TileCoord, TileGrid};

pub struct Camera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub vp_w: u32,
    pub vp_h: u32,
    pub img_w: u32,
    pub img_h: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self { scale_x: 2.0, scale_y: -2.0, offset_x: -1.0, offset_y: 1.0 }
    }
}

impl Camera {
    pub fn new(vp_w: u32, vp_h: u32, img_w: u32, img_h: u32) -> Self {
        Self { pan_x: 0.0, pan_y: 0.0, zoom: 1.0, vp_w, vp_h, img_w, img_h }
    }

    pub fn resize(&mut self, vp_w: u32, vp_h: u32) {
        self.vp_w = vp_w;
        self.vp_h = vp_h;
    }

    pub fn fit(&mut self) {
        let w = self.img_w as f32;
        let h = self.img_h as f32;
        self.zoom = f32::min(self.vp_w as f32 / w, self.vp_h as f32 / h);
        self.pan_x = -(self.vp_w as f32 / self.zoom - w) / 2.0;
        self.pan_y = -(self.vp_h as f32 / self.zoom - h) / 2.0;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x -= dx / self.zoom;
        self.pan_y -= dy / self.zoom;
    }

    pub fn zoom_at(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        let a_img_x = anchor_x * self.vp_w as f32 / self.zoom + self.pan_x;
        let a_img_y = anchor_y * self.vp_h as f32 / self.zoom + self.pan_y;
        self.zoom = (self.zoom * factor).clamp(0.02, 100.0);
        self.pan_x = a_img_x - anchor_x * self.vp_w as f32 / self.zoom;
        self.pan_y = a_img_y - anchor_y * self.vp_h as f32 / self.zoom;
    }

    pub fn to_uniform(&self) -> CameraUniform {
        let vp_w = self.vp_w as f32;
        let vp_h = self.vp_h as f32;
        CameraUniform {
            scale_x: 2.0 * self.zoom / vp_w,
            scale_y: -2.0 * self.zoom / vp_h,
            offset_x: -1.0 - 2.0 * self.pan_x * self.zoom / vp_w,
            offset_y: 1.0 + 2.0 * self.pan_y * self.zoom / vp_h,
        }
    }

    pub fn visible_tiles(&self, tile_size: u32) -> Vec<TileCoord> {
        let (vx, vy, vw, vh) = self.visible_bounds();
        let tx_min = (vx.floor() as u32 / tile_size).max(0);
        let ty_min = (vy.floor() as u32 / tile_size).max(0);
        let tx_max = ((vx + vw).ceil() as u32 / tile_size).min(self.img_w.div_ceil(tile_size));
        let ty_max = ((vy + vh).ceil() as u32 / tile_size).min(self.img_h.div_ceil(tile_size));

        let mut tiles = Vec::new();
        for ty in ty_min..=ty_max {
            for tx in tx_min..=tx_max {
                tiles.push(TileCoord::new(0, tx, ty, tile_size, self.img_w, self.img_h));
            }
        }
        tiles
    }

    pub fn visible_bounds(&self) -> (f32, f32, f32, f32) {
        let w = self.vp_w as f32 / self.zoom;
        let h = self.vp_h as f32 / self.zoom;
        (self.pan_x, self.pan_y, w, h)
    }
}
