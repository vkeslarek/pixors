use pixors_executor::source::TileRange;

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
    pub mip_level: f32,
    /// MIP-0 (original) image width — needed for exact coordinate mapping.
    pub img_w0: f32,
    /// MIP-0 (original) image height.
    pub img_h0: f32,
    pub _pad0: f32,
    pub _pad1: f32,
}

pub fn compute_max_mip(width: u32, height: u32) -> u32 {
    let max_dim = width.max(height);
    if max_dim <= 256 {
        return 0;
    }
    let levels = (max_dim as f64 / 256.0).log2().floor() as u32;
    levels.min(8)
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
        self.zoom = f32::min(self.vp_w / self.img_w, self.vp_h / self.img_h) * 0.95;
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
        let fit = f32::min(self.vp_w / self.img_w, self.vp_h / self.img_h);
        let min_zoom = (fit * 0.2).max(1.0 / 512.0);
        self.zoom = (self.zoom * factor).clamp(min_zoom, 64.0);
        self.pan_x = a_img_x - anchor_x / self.zoom;
        self.pan_y = a_img_y - anchor_y / self.zoom;
    }

    /// Select a MIP level appropriate for the current zoom.
    ///
    /// Two constraints:
    /// 1. Zoom-driven ideal: `mip = floor(-log₂(z))` — one texel ≈ one screen pixel.
    /// 2. Texture cap:     texture dimension must stay ≤ 8192 px (wgpu + sane tile count).
    ///
    /// The higher of the two wins (lower quality), so enormous images never
    /// create a multi-gigabyte texture at MIP 0, but smaller images degrade
    /// gracefully to MIP 0 when the user zooms in.
    pub fn visible_mip_level(&self) -> u32 {
        // ── Zoom-driven (standard graphics formula) ─────────────────────────
        let zoom_mip = if self.zoom >= 0.5 {
            0
        } else {
            (-(self.zoom as f64).log2().floor() as u32)
                .saturating_sub(1) // bias toward higher quality
                .min(compute_max_mip(self.img_w as u32, self.img_h as u32))
        };

        // ── Texture cap (keep tile count + VRAM under control) ─────────────
        const MAX_TEX_DIM: u32 = 8192;
        let max_img_dim = self.img_w.max(self.img_h) as u32;
        let floor_mip = if max_img_dim <= MAX_TEX_DIM {
            0
        } else {
            ((max_img_dim as f64 / MAX_TEX_DIM as f64).log2().ceil() as u32)
                .min(compute_max_mip(max_img_dim, max_img_dim))
        };

        zoom_mip.max(floor_mip)
    }

    /// Tile indices visible at the given MIP level, with 1-tile padding for smooth scroll.
    pub fn visible_tile_range(&self, mip: u32, tile_size: u32) -> TileRange {
        let mip_scale = (1u32 << mip) as f32;
        let ts = tile_size as f32;

        let x0 = self.pan_x.max(0.0);
        let y0 = self.pan_y.max(0.0);
        let x1 = (self.pan_x + self.vp_w / self.zoom).min(self.img_w);
        let y1 = (self.pan_y + self.vp_h / self.zoom).min(self.img_h);

        let mip_w = (self.img_w as u32 >> mip).max(1);
        let mip_h = (self.img_h as u32 >> mip).max(1);
        let ntx = mip_w.div_ceil(tile_size);
        let nty = mip_h.div_ceil(tile_size);

        let tx_start = ((x0 / mip_scale / ts).floor() as u32).saturating_sub(1);
        let ty_start = ((y0 / mip_scale / ts).floor() as u32).saturating_sub(1);
        let tx_end = (((x1 / mip_scale / ts).ceil() as u32) + 1).min(ntx);
        let ty_end = (((y1 / mip_scale / ts).ceil() as u32) + 1).min(nty);

        TileRange { tx_start, tx_end, ty_start, ty_end }
    }

    pub fn to_uniform(&self, mip_level: u32) -> CameraUniform {
        let mip_w = (self.img_w as u32 >> mip_level).max(1) as f32;
        let mip_h = (self.img_h as u32 >> mip_level).max(1) as f32;
        CameraUniform {
            vp_w: self.vp_w,
            vp_h: self.vp_h,
            img_w: mip_w,
            img_h: mip_h,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            zoom: self.zoom,
            mip_level: mip_level as f32,
            img_w0: self.img_w,
            img_h0: self.img_h,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}
