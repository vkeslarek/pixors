#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub vp_w: f32,
    pub vp_h: f32,
    pub img_w: f32,
    pub img_h: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub _pad: f32,
}

pub struct Camera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub vp_w: f32,
    pub vp_h: f32,
    pub img_w: f32,
    pub img_h: f32,
}

impl Camera {
    pub fn new(img_w: f32, img_h: f32) -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            vp_w: 1.0,
            vp_h: 1.0,
            img_w,
            img_h,
        }
    }

    pub fn resize(&mut self, vp_w: f32, vp_h: f32) {
        self.vp_w = vp_w;
        self.vp_h = vp_h;
    }

    pub fn fit(&mut self) {
        self.zoom = f32::min(
            self.vp_w / self.img_w,
            self.vp_h / self.img_h,
        ) * 0.95;
        self.pan_x = -(self.vp_w / self.zoom - self.img_w) / 2.0;
        self.pan_y = -(self.vp_h / self.zoom - self.img_h) / 2.0;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x -= dx / self.zoom;
        self.pan_y -= dy / self.zoom;
    }

    pub fn zoom_at(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        let a_img_x = anchor_x / self.zoom + self.pan_x;
        let a_img_y = anchor_y / self.zoom + self.pan_y;
        // Min zoom: image never smaller than ~20% of viewport. Max zoom: 64× (pixel-peep).
        let fit = f32::min(self.vp_w / self.img_w, self.vp_h / self.img_h);
        let min_zoom = (fit * 0.2).max(1.0 / 512.0);
        self.zoom = (self.zoom * factor).clamp(min_zoom, 64.0);
        self.pan_x = a_img_x - anchor_x / self.zoom;
        self.pan_y = a_img_y - anchor_y / self.zoom;
    }

    pub fn to_uniform(&self) -> CameraUniform {
        CameraUniform {
            vp_w: self.vp_w,
            vp_h: self.vp_h,
            img_w: self.img_w,
            img_h: self.img_h,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            zoom: self.zoom,
            _pad: 0.0,
        }
    }
}
