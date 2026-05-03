use pixors_engine::image::{Tile, TileCoord, TileGrid};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::source::Source;
use pixors_engine::pixel::Rgba;
use half::f16;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const SQUARE_SIZE: u32 = 64;

pub struct CheckerboardSource {
    pub img_w: u32,
    pub img_h: u32,
    pub tile_size: u32,
}

impl Source for CheckerboardSource {
    type Item = Tile<Rgba<f16>>;

    fn run(self, emit: &mut Emitter<Self::Item>, _cancel: Arc<AtomicBool>) {
        let grid = TileGrid::new(self.img_w, self.img_h, self.tile_size);
        for coord in grid.tiles() {
            if coord.width == 0 || coord.height == 0 {
                continue;
            }
            let count = coord.pixel_count();
            let mut pixels = vec![Rgba::black(); count];

            for y in 0..coord.height {
                let gy = coord.py + y;
                for x in 0..coord.width {
                    let gx = coord.px + x;
                    let sq_x = gx / SQUARE_SIZE;
                    let sq_y = gy / SQUARE_SIZE;
                    let c = if (sq_x + sq_y) % 2 == 0 {
                        Rgba::white()
                    } else {
                        Rgba::new(
                            f16::from_f32(0.1),
                            f16::from_f32(0.1),
                            f16::from_f32(0.1),
                            f16::ONE,
                        )
                    };
                    pixels[(y * coord.width + x) as usize] = c;
                }
            }

            emit.emit(Tile::new(*coord, pixels));
        }
    }

    fn total(&self) -> Option<u32> {
        Some((self.img_w.div_ceil(self.tile_size) * self.img_h.div_ceil(self.tile_size)) as u32)
    }
}
