use crate::image::{Neighborhood, Tile};
use crate::pixel::PixelAccumulator;
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use std::sync::{Arc, Mutex};

pub struct BoxBlurOp<P: PixelAccumulator> {
    pub radius_x: u32,
    pub radius_y: u32,
    _phantom: std::marker::PhantomData<P>,
}

impl<P: PixelAccumulator> BoxBlurOp<P> {
    pub fn new(radius_x: u32, radius_y: u32) -> Self {
        Self { radius_x, radius_y, _phantom: std::marker::PhantomData }
    }

    fn blur_rows(src: &[P], dst: &mut [P], w: usize, r: usize) {
        use rayon::prelude::*;
        let len = 2 * r + 1;
        dst.par_chunks_exact_mut(w).zip(src.par_chunks_exact(w)).for_each(|(row_dst, row_src)| {
            let mut sum = P::Sum::default();
            for x in 0..r.min(w) { row_src[x].accumulate(&mut sum); }
            for x in 0..w {
                let add_idx = (x + r).min(w - 1);
                if x > 0 { row_src[add_idx].accumulate(&mut sum); }
                if x > r {
                    let rm_idx = x - r - 1;
                    P::subtract(&mut sum, &row_src[rm_idx]);
                }
                let count = ((add_idx + 1).saturating_sub(x.saturating_sub(r)) as u32).min(len as u32).max(1);
                row_dst[x] = P::from_sum(sum, count);
            }
        });
    }

    fn blur_cols(src: &[P], dst: &mut [P], w: usize, h: usize, r: usize) {
        use rayon::prelude::*;
        let len = 2 * r + 1;
        let dst = Mutex::new(dst);
        (0..w).into_par_iter().for_each(|x| {
            let mut sum = P::Sum::default();
            for y in 0..r.min(h) { src[y * w + x].accumulate(&mut sum); }
            for y in 0..h {
                let add_idx = (y + r).min(h - 1);
                if y > 0 { src[add_idx * w + x].accumulate(&mut sum); }
                if y > r {
                    let rm_idx = y - r - 1;
                    P::subtract(&mut sum, &src[rm_idx * w + x]);
                }
                let count = ((add_idx + 1).saturating_sub(y.saturating_sub(r)) as u32).min(len as u32).max(1);
                let d = &mut dst.lock().unwrap()[y * w + x];
                *d = P::from_sum(sum, count);
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
    type Out = Arc<Tile<P>>;

    fn name(&self) -> &'static str { "box_blur" }

    fn process(&mut self, nbhd: Self::In, emit: &mut Emitter<Self::Out>) -> Result<(), crate::error::Error> {
        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let rx = self.radius_x;
        let ry = self.radius_y;

        let rw = cw + 2 * rx;
        let rh = ch + 2 * ry;
        let rox = cx.saturating_sub(rx);
        let roy = cy.saturating_sub(ry);

        let default = P::from_sum(P::Sum::default(), 1);
        let mut src = vec![default; (rw * rh) as usize];
        for y in 0..rh {
            for x in 0..rw {
                src[(y * rw + x) as usize] = nbhd.pixel_at(rox + x, roy + y).cloned().unwrap_or(default);
            }
        }

        let mut tmp = vec![default; src.len()];
        Self::blur_rows(&src, &mut tmp, rw as usize, rx as usize);

        let mut dst = vec![default; src.len()];
        Self::blur_cols(&tmp, &mut dst, rw as usize, rh as usize, ry as usize);

        let mut tile_pixels = Vec::with_capacity((cw * ch) as usize);
        for y in 0..ch {
            for x in 0..cw {
                let sx = (cx - rox + x) as usize;
                let sy = (cy - roy + y) as usize;
                tile_pixels.push(dst[sy * rw as usize + sx].clone());
            }
        }

        emit.emit(Arc::new(Tile::new(nbhd.center, tile_pixels)));
        Ok(())
    }
}
