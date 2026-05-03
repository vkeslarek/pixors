use serde::{Deserialize, Serialize};

use crate::data::Tile;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::graph::runner::OperationRunner;
use crate::data::Device;
use crate::stage::Stage;
use crate::error::Error;
use crate::gpu::Buffer;
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlurKernel {
    pub radius: u32,
}

impl Stage for BlurKernel {
    fn kind(&self) -> &'static str {
        "blur"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(BlurKernelRunner::new(self.radius)))
    }
}

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
        let _sw = debug_stopwatch!("blur");
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

        let rw = (cw + 2 * r) as usize;
        let rh = (ch + 2 * r) as usize;
        let rox = cx.saturating_sub(r);
        let roy = cy.saturating_sub(r);

        let mut src = vec![0u8; rw * rh * bpp];

        for tile in &nbhd.tiles {
            let tile_data: &[u8] = match &tile.data {
                Buffer::Cpu(v) => v.as_slice(),
                Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
            };
            let tw = tile.coord.width as usize;
            let tpx = tile.coord.px;
            let tpy = tile.coord.py;

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

        let blurred = box_blur_rgba8(&src, rw, rh, r as usize);

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
            Buffer::cpu(tile_data),
        )));
        Ok(())
    }
}

fn box_blur_rgba8(data: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }

    let stride = w * 4;
    let hpass = blur_axis(data, h, stride, w, 4, r);
    blur_axis(&hpass, w, 4, h, stride, r)
}

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
                let new_i = i + r;
                if new_i < axis_len {
                    let off = line_origin + new_i * step;
                    for c in 0..4 {
                        sum[c] += data[off + c] as u32;
                    }
                    count += 1;
                }
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
