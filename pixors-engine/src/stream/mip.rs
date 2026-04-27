use crate::image::TileCoord;
use crate::stream::{Frame, FrameKind, FrameMeta, Pipe};
use std::collections::HashMap;
use std::sync::mpsc;

/// Recursive downsampling pipe: receives MIP-0 tiles, emits all MIP levels (0..max).
/// Uses 2×2 box filter internally. A single pipe replaces N chained pipes.
pub struct MipPipe {
    tile_size: u32,
    max_levels: u32,
}

impl MipPipe {
    pub fn new(tile_size: u32, max_levels: u32) -> Self { Self { tile_size, max_levels } }
}

impl Pipe for MipPipe {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame> {
        let (tx, out) = mpsc::sync_channel(64);
        let tile_size = self.tile_size;
        let max_levels = self.max_levels;

        std::thread::spawn(move || {
            // Key: (src_mip, dst_tx, dst_ty) → [Option<Frame>; 4] — isolated per MIP level
            let mut accum: HashMap<(u32, u32, u32), [Option<Frame>; 4]> = HashMap::new();
            // Queue of newly generated tiles that may trigger further levels
            let mut pending: Vec<Frame> = Vec::new();

            loop {
                let frame = if let Some(f) = pending.pop() {
                    f
                } else {
                    match rx.recv() {
                        Ok(f) => f,
                        Err(_) => break,
                    }
                };

                if frame.is_terminal() {
                    tx.send(frame).ok();
                    break;
                }

                match &frame.kind {
                    FrameKind::Tile { coord } => {
                        let src_mip = frame.meta.mip_level;

                        // Always pass through
                        if tx.send(frame.clone()).is_err() { return; }

                        // Extract needed values before frame is moved into accumulator
                        let meta_layer = frame.meta.layer_id;
                        let meta_image_w = frame.meta.image_w;
                        let meta_image_h = frame.meta.image_h;
                        let meta_cs = frame.meta.color_space;
                        let meta_gen = frame.meta.generation;

                        // Try to generate next level if within bounds
                        if src_mip < max_levels {
                            let dst_mip = src_mip + 1;
                            let dst_tx = coord.tx / 2;
                            let dst_ty = coord.ty / 2;
                            let qi = ((coord.ty % 2) * 2 + (coord.tx % 2)) as usize;

                            let key = (src_mip, dst_tx, dst_ty);
                            let entry = accum.entry(key).or_insert([None, None, None, None]);
                            entry[qi] = Some(frame);

                            // Check if 2×2 quadrant is complete
                            let src_tiles_x = meta_image_w.div_ceil(tile_size);
                            let src_tiles_y = meta_image_h.div_ceil(tile_size);
                            let req_w = if dst_tx * 2 + 1 < src_tiles_x { 2 } else { 1 };
                            let req_h = if dst_ty * 2 + 1 < src_tiles_y { 2 } else { 1 };

                            let complete = (0..req_h).all(|dy| (0..req_w).all(|dx| entry[(dy * 2 + dx) as usize].is_some()));

                            if complete {
                                let dst_iw = (meta_image_w >> 1).max(1);
                                let dst_ih = (meta_image_h >> 1).max(1);

                                let actual_w = if dst_tx * tile_size < dst_iw {
                                    (dst_iw - dst_tx * tile_size).min(tile_size)
                                } else { 0 };
                                let actual_h = if dst_ty * tile_size < dst_ih {
                                    (dst_ih - dst_ty * tile_size).min(tile_size)
                                } else { 0 };

                                if actual_w > 0 && actual_h > 0 {
                                    let n = (actual_w * actual_h) as usize;
                                    let mut out = vec![0u8; n * 4];

                                    for dy in 0..actual_h {
                                        for dx in 0..actual_w {
                                            let mut r = 0u32; let mut g = 0u32; let mut b = 0u32; let mut a = 0u32;
                                            let mut count = 0u32;
                                            for sy in 0..2u32 {
                                                for sx in 0..2u32 {
                                                    let sx_abs = dx * 2 + sx;
                                                    let sy_abs = dy * 2 + sy;
                                                    let sti = match (sx_abs >= tile_size, sy_abs >= tile_size) {
                                                        (false, false) => 0, (true, false) => 1,
                                                        (false, true) => 2, (true, true) => 3,
                                                    };
                                                    if let Some(st) = entry[sti].as_ref() {
                                                        let lx = sx_abs % tile_size;
                                                        let ly = sy_abs % tile_size;
                                                        let (sw, sh) = match &st.kind {
                                                            FrameKind::Tile { coord } => (coord.width, coord.height),
                                                            _ => (tile_size, tile_size),
                                                        };
                                                        if lx < sw && ly < sh {
                                                            let off = (ly * sw + lx) as usize * 4;
                                                            let d = st.data.as_ref();
                                                            if off + 3 < d.len() {
                                                                r += d[off] as u32; g += d[off+1] as u32;
                                                                b += d[off+2] as u32; a += d[off+3] as u32;
                                                                count += 1;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            let oi = (dy * actual_w + dx) as usize * 4;
                                            let div = count.max(1);
                                            out[oi]= (r/div) as u8; out[oi+1]= (g/div) as u8;
                                            out[oi+2]= (b/div) as u8; out[oi+3]= (a/div) as u8;
                                        }
                                    }

                                    let dst_coord = TileCoord::new(dst_mip, dst_tx, dst_ty, tile_size, dst_iw, dst_ih);
                                    let new_frame = Frame::new(
                                        FrameMeta {
                                            layer_id: meta_layer,
                                            mip_level: dst_mip,
                                            image_w: dst_iw,
                                            image_h: dst_ih,
                                            color_space: meta_cs,
                                            total_tiles: 0,
                                            generation: meta_gen,
                                        },
                                        FrameKind::Tile { coord: dst_coord },
                                        out,
                                    );
                                    pending.push(new_frame);
                                }
                                // Clear quadrant
                                entry[0]=None; entry[1]=None; entry[2]=None; entry[3]=None;
                            }
                        }
                    }
                    _ => { tx.send(frame).ok(); }
                }
            }
        });
        out
    }
}
