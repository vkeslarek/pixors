use crate::container::Tile;
use crate::egraph::emitter::Emitter;
use crate::egraph::item::Item;
use crate::egraph::runner::OperationRunner;
use crate::error::Error;
use crate::storage::Buffer;

/// Box-blur kernel. Operates on RGBA8 neighborhoods: gathers the source
/// region (center tile plus padding from neighbours) and emits a blurred
/// center tile.
pub struct BlurKernelRunner {
    radius: u32,
}

impl BlurKernelRunner {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl OperationRunner for BlurKernelRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("expected Neighborhood")),
        };

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let r = self.radius;
        let bpp = 4usize;

        // Source region: center tile plus `radius` pixels of padding on each
        // side. Out-of-image pixels stay zero (clamp-to-zero edge mode).
        let rw = (cw + 2 * r) as usize;
        let rh = (ch + 2 * r) as usize;
        let rox = cx.saturating_sub(r);
        let roy = cy.saturating_sub(r);

        let mut src = vec![0u8; rw * rh * bpp];

        // Copy each neighbouring tile's pixels into the source buffer.
        for tile in &nbhd.tiles {
            let tile_data = match &tile.data {
                Buffer::Cpu(v) => v,
                Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
            };
            let tw = tile.coord.width as usize;
            let tpx = tile.coord.px;
            let tpy = tile.coord.py;

            // Intersection between the tile's pixel bounds and the source
            // region's pixel bounds, in absolute coordinates.
            let x0 = rox.max(tpx);
            let y0 = roy.max(tpy);
            let x1 = (rox + rw as u32).min(tpx + tile.coord.width);
            let y1 = (roy + rh as u32).min(tpy + tile.coord.height);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }

            let copy_w = (x1 - x0) as usize;
            for abs_y in y0..y1 {
                let src_row = (abs_y - tpy) as usize;
                let src_col = (x0 - tpx) as usize;
                let dst_row = (abs_y - roy) as usize;
                let dst_col = (x0 - rox) as usize;

                let src_off = (src_row * tw + src_col) * bpp;
                let dst_off = (dst_row * rw + dst_col) * bpp;
                let len = copy_w * bpp;

                if src_off + len > tile_data.len() || dst_off + len > src.len() {
                    continue;
                }
                src[dst_off..dst_off + len].copy_from_slice(&tile_data[src_off..src_off + len]);
            }
        }

        // Separable box blur on the padded source.
        let blurred = box_blur_rgba8(&src, rw, rh, r as usize);

        // Extract the center-aligned slice that corresponds to the original
        // center tile.
        let cw_u = cw as usize;
        let ch_u = ch as usize;
        let off_x = (cx - rox) as usize;
        let off_y = (cy - roy) as usize;
        let mut tile_data = Vec::with_capacity(cw_u * ch_u * bpp);
        for y in 0..ch_u {
            let row_off = ((off_y + y) * rw + off_x) * bpp;
            tile_data.extend_from_slice(&blurred[row_off..row_off + cw_u * bpp]);
        }

        emit.emit(Item::Tile(Tile::new(
            nbhd.center,
            nbhd.meta,
            Buffer::Cpu(tile_data),
        )));
        Ok(())
    }
}

/// Two-pass separable box blur on RGBA8 data.
///
/// For each output pixel at column `x`, the kernel averages all input pixels
/// in `[x - r, x + r]`, clamped to `[0, w - 1]`. The same is done vertically
/// on the result of the horizontal pass.
fn box_blur_rgba8(data: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }

    let stride = w * 4;
    // Horizontal pass: each row is one line; samples step by 4 bytes.
    let hpass = blur_axis(data, /*lines=*/ h, /*line_step=*/ stride, /*axis_len=*/ w, /*step=*/ 4, r);
    // Vertical pass: each column is one line; samples step by `stride` bytes.
    blur_axis(&hpass, /*lines=*/ w, /*line_step=*/ 4, /*axis_len=*/ h, /*step=*/ stride, r)
}

/// Sliding-window average along one axis of an RGBA8 image.
///
/// Generic over the two passes: a "line" is either an image row (horizontal
/// pass, `step = 4`) or an image column (vertical pass, `step = stride`).
fn blur_axis(
    data: &[u8],
    lines: usize,
    line_step: usize,
    axis_len: usize,
    step: usize,
    r: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; data.len()];

    for line in 0..lines {
        let line_origin = line * line_step;
        let mut sum = [0u32; 4];
        let mut count = 0u32;

        // Initial window: positions [0, r] (clipped to axis).
        let initial_end = r.min(axis_len - 1);
        for i in 0..=initial_end {
            let off = line_origin + i * step;
            for c in 0..4 {
                sum[c] += data[off + c] as u32;
            }
            count += 1;
        }

        for i in 0..axis_len {
            if i > 0 {
                // Add the pixel entering the window on the right (if any).
                let new_i = i + r;
                if new_i < axis_len {
                    let off = line_origin + new_i * step;
                    for c in 0..4 {
                        sum[c] += data[off + c] as u32;
                    }
                    count += 1;
                }
                // Drop the pixel leaving the window on the left (if it was
                // ever inside it).
                if i > r {
                    let old_i = i - r - 1;
                    let off = line_origin + old_i * step;
                    for c in 0..4 {
                        sum[c] -= data[off + c] as u32;
                    }
                    count -= 1;
                }
            }
            let dst_off = line_origin + i * step;
            for c in 0..4 {
                dst[dst_off + c] = (sum[c] / count) as u8;
            }
        }
    }

    dst
}
