use pixors_engine::image::{Neighborhood, Tile};
use pixors_engine::pixel::PixelAccumulator;
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use std::sync::Arc;

pub struct BoxBlurOp<P: PixelAccumulator> {
    pub radius_x: u32,
    pub radius_y: u32,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: PixelAccumulator> BoxBlurOp<P> {
    pub fn new(radius_x: u32, radius_y: u32) -> Self {
        Self { radius_x, radius_y, _phantom: std::marker::PhantomData }
    }

    /// Sliding-window box blur along rows. `src` and `dst` are w*h, row-major.
    fn blur_rows(src: &[P], dst: &mut [P], w: usize, r: usize) {
        use rayon::prelude::*;
        if w == 0 { return; }
        if r == 0 {
            dst.copy_from_slice(src);
            return;
        }
        dst.par_chunks_exact_mut(w).zip(src.par_chunks_exact(w)).for_each(|(row_dst, row_src)| {
            let mut sum = P::Sum::default();
            let init = r.min(w - 1);
            for i in 0..=init { row_src[i].accumulate(&mut sum); }
            for x in 0..w {
                let l = x.saturating_sub(r);
                let h = (x + r).min(w - 1);
                let count = (h - l + 1) as u32;
                row_dst[x] = P::from_sum(sum, count);
                if x + 1 < w {
                    let add = x + r + 1;
                    if add < w { row_src[add].accumulate(&mut sum); }
                    if x >= r { P::subtract(&mut sum, &row_src[x - r]); }
                }
            }
        });
    }

    /// Sliding-window box blur along columns. Lock-free: each parallel iteration
    /// writes to a disjoint column, so raw-pointer writes are safe.
    fn blur_cols(src: &[P], dst: &mut [P], w: usize, h: usize, r: usize) {
        use rayon::prelude::*;
        if w == 0 || h == 0 { return; }
        if r == 0 {
            dst.copy_from_slice(src);
            return;
        }
        let dst_ptr = dst.as_mut_ptr() as usize;
        (0..w).into_par_iter().for_each(|x| {
            let ptr = dst_ptr as *mut P;
            let mut sum = P::Sum::default();
            let init = r.min(h - 1);
            for i in 0..=init { src[i * w + x].accumulate(&mut sum); }
            for y in 0..h {
                let l = y.saturating_sub(r);
                let hh = (y + r).min(h - 1);
                let count = (hh - l + 1) as u32;
                // SAFETY: each x maps to a disjoint set of indices (y*w+x for all y),
                // and parallel iterations have distinct x.
                unsafe { ptr.add(y * w + x).write(P::from_sum(sum, count)); }
                if y + 1 < h {
                    let add = y + r + 1;
                    if add < h { src[add * w + x].accumulate(&mut sum); }
                    if y >= r { P::subtract(&mut sum, &src[(y - r) * w + x]); }
                }
            }
        });
    }
}

impl<P> Operation for BoxBlurOp<P>
where
    P: PixelAccumulator,
    Neighborhood<P>: Send + 'static,
{
    type In = Neighborhood<P>;
    type Out = Tile<P>;

    fn name(&self) -> &'static str { "box_blur" }

    fn process(&mut self, nbhd: Arc<Self::In>, emit: &mut Emitter<Self::Out>) -> Result<(), pixors_engine::error::Error> {
        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let mip = nbhd.center.mip_level;
        let rx = self.radius_x >> mip;
        let ry = self.radius_y >> mip;

        let rw = (cw + 2 * rx) as usize;
        let rh = (ch + 2 * ry) as usize;
        let rox = cx.saturating_sub(rx);
        let roy = cy.saturating_sub(ry);

        let default = P::from_sum(P::Sum::default(), 1);

        // Build source rect by copying tile slices contiguously rather than
        // calling pixel_at per pixel (avoids div/mod + HashMap lookup per pixel).
        let mut src = vec![default; rw * rh];
        let r_tx0 = rox / nbhd.tile_size;
        let r_ty0 = roy / nbhd.tile_size;
        let r_tx1 = (rox + rw as u32 - 1) / nbhd.tile_size;
        let r_ty1 = (roy + rh as u32 - 1) / nbhd.tile_size;
        for ty in r_ty0..=r_ty1 {
            for tx in r_tx0..=r_tx1 {
                let dtx = tx as i32 - nbhd.center.tx as i32;
                let dty = ty as i32 - nbhd.center.ty as i32;
                let tile_opt = match nbhd.tile_at_offset(dtx, dty) {
                    Some(Some(t)) => Some(t),
                    _ => None,
                };
                if let Some(tile) = tile_opt {
                    let tpx = tile.coord.px;
                    let tpy = tile.coord.py;
                    let tw = tile.coord.width;
                    let th = tile.coord.height;
                    let x0 = rox.max(tpx);
                    let y0 = roy.max(tpy);
                    let x1 = (rox + rw as u32).min(tpx + tw);
                    let y1 = (roy + rh as u32).min(tpy + th);
                    if x1 <= x0 || y1 <= y0 { continue; }
                    let twu = tw as usize;
                    for py in y0..y1 {
                        let dst_row = (py - roy) as usize * rw;
                        let dst_col = (x0 - rox) as usize;
                        let lx = (x0 - tpx) as usize;
                        let ly = (py - tpy) as usize;
                        let n = (x1 - x0) as usize;
                        let src_off = ly * twu + lx;
                        src[dst_row + dst_col .. dst_row + dst_col + n]
                            .copy_from_slice(&tile.data[src_off .. src_off + n]);
                    }
                } else {
                    // Edge / missing tile — fall back to pixel_at (Clamp / Mirror / Transparent).
                    let tpx = tx * nbhd.tile_size;
                    let tpy = ty * nbhd.tile_size;
                    let x0 = rox.max(tpx);
                    let y0 = roy.max(tpy);
                    let x1 = (rox + rw as u32).min(tpx + nbhd.tile_size);
                    let y1 = (roy + rh as u32).min(tpy + nbhd.tile_size);
                    for py in y0..y1 {
                        for px in x0..x1 {
                            let idx = (py - roy) as usize * rw + (px - rox) as usize;
                            src[idx] = nbhd.pixel_at(px, py).copied().unwrap_or(default);
                        }
                    }
                }
            }
        }

        // Two-pass separable blur: rows → cols. Reuse `src` as final buffer.
        let mut tmp = vec![default; rw * rh];
        Self::blur_rows(&src, &mut tmp, rw, rx as usize);
        Self::blur_cols(&tmp, &mut src, rw, rh, ry as usize);

        // Extract center tile from the blurred buffer.
        let cw_us = cw as usize;
        let ch_us = ch as usize;
        let off_x = (cx - rox) as usize;
        let off_y = (cy - roy) as usize;
        let mut tile_pixels = Vec::with_capacity(cw_us * ch_us);
        for y in 0..ch_us {
            let row_off = (off_y + y) * rw + off_x;
            tile_pixels.extend_from_slice(&src[row_off .. row_off + cw_us]);
        }

        emit.emit(Tile::new(nbhd.center, tile_pixels));
        Ok(())
    }
}
