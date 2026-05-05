//! MIP pyramid with lazy tile-store-backed generation.

use crate::error::Error;
use crate::model::image::{TileCoord, TileGrid};
use crate::model::pixel::Rgba;
use crate::model::storage::WorkingWriter;
use half::f16;
use std::path::{Path, PathBuf};

/// One level in a MIP pyramid with its own tile storage.
#[derive(Debug)]
pub struct MipLevel {
    /// Level index (0 = full resolution, 1 = half, etc.)
    pub index: usize,
    /// Image width at this level (pixels).
    pub width: u32,
    /// Image height at this level (pixels).
    pub height: u32,
    /// Scale factor relative to full resolution (0.5, 0.25, ...)
    pub scale: f32,
    /// Tile grid for this level.
    pub tile_grid: TileGrid,
    /// Tile store for this level (disk-backed).
    pub tile_store: WorkingWriter,
    /// Whether this level has been generated (tiles populated).
    pub generated: bool,
}

impl MipLevel {
    /// Creates a new MIP level with empty tile store.
    pub fn new(
        index: usize,
        width: u32,
        height: u32,
        scale: f32,
        tile_size: u32,
        base_dir: PathBuf,
    ) -> Result<Self, Error> {
        let tile_grid = TileGrid::new(width, height, tile_size);
        // Each MIP level gets its own subdirectory to prevent tile file collisions.
        let tile_store = WorkingWriter::new_with_subdir(
            base_dir,
            &format!("mip_{}", index),
            tile_size,
            width,
            height,
        )?;

        Ok(Self {
            index,
            width,
            height,
            scale,
            tile_grid,
            tile_store,
            generated: false,
        })
    }

    /// Returns tile at given tile coordinates (tx, ty).
    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&TileCoord> {
        self.tile_grid.tile_at(tx, ty)
    }

    /// Returns all tiles in this level.
    pub fn tiles(&self) -> impl Iterator<Item = &TileCoord> {
        self.tile_grid.tiles()
    }
}

/// Pre-computed downscaled versions of the image for fast zoom-out rendering.
/// Each level has its own tile store for lazy generation.
#[derive(Debug)]
pub struct MipPyramid {
    levels: Vec<MipLevel>,
}

impl MipPyramid {
    /// Creates a new MIP pyramid with empty tile stores.
    /// Level 0 is implicit (full resolution), not stored here.
    pub fn new(
        src_width: u32,
        src_height: u32,
        tile_size: u32,
        base_dir: PathBuf,
    ) -> Result<Self, Error> {
        let mut levels = Vec::new();
        let mut width = src_width.max(1);
        let mut height = src_height.max(1);
        let mut scale = 0.5_f32;
        let mut index = 1;

        while width > 1 || height > 1 {
            width = (width / 2).max(1);
            height = (height / 2).max(1);
            let level = MipLevel::new(index, width, height, scale, tile_size, base_dir.clone())?;
            levels.push(level);
            scale *= 0.5;
            index += 1;
        }

        Ok(Self { levels })
    }

    /// Returns the number of MIP levels (excluding level 0).
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Returns a reference to a specific MIP level (1-indexed).
    pub fn level(&self, index: usize) -> Option<&MipLevel> {
        self.levels.get(index.saturating_sub(1))
    }

    /// Returns a mutable reference to a specific MIP level (1-indexed).
    pub fn level_mut(&mut self, index: usize) -> Option<&mut MipLevel> {
        self.levels.get_mut(index.saturating_sub(1))
    }

    /// Returns all levels.
    pub fn levels(&self) -> &[MipLevel] {
        &self.levels
    }

    /// Returns all levels as mutable.
    pub fn levels_mut(&mut self) -> &mut [MipLevel] {
        &mut self.levels
    }

    /// Replace all levels with freshly generated ones (e.g. after lock-free generation).
    pub fn replace_levels(&mut self, new_levels: Vec<MipLevel>) {
        self.levels = new_levels;
    }

    /// Consume the pyramid and return its levels.
    pub fn into_levels(self) -> Vec<MipLevel> {
        self.levels
    }

    /// Ensure a specific MIP level is generated.
    pub fn is_level_ready(&self, level: usize) -> bool {
        self.level(level).is_some_and(|l| l.generated)
    }

    /// Select the appropriate MIP level for the given zoom.
    pub fn level_for_zoom(zoom: f32) -> usize {
        if zoom >= 0.5 {
            return 0;
        }
        (-(zoom.max(1e-6).log2())).floor() as usize
    }

    /// Generate all MIP levels from MIP 0 tile store.
    /// Runs box-filter downsampling in parallel via rayon.
    pub fn generate_from_mip0(mip0: &WorkingWriter, base_dir: &Path) -> Result<Self, Error> {
        let _sw = crate::debug_stopwatch!("generate_from_mip0");
        let tile_size = mip0.tile_size();
        let mut width = mip0.image_width();
        let mut height = mip0.image_height();

        tracing::debug!(
            "generate_from_mip0: {}x{} tile_size={}",
            width,
            height,
            tile_size
        );

        let mut levels: Vec<MipLevel> = Vec::new();
        let mut level_idx = 1u32;

        while width > 1 || height > 1 {
            width = (width + 1).max(1) / 2;
            height = (height + 1).max(1) / 2;

            let store = WorkingWriter::new_with_subdir(
                base_dir.to_path_buf(),
                &format!("mip_{}", level_idx),
                tile_size,
                width,
                height,
            )?;
            downsample_level_rayon(
                levels
                    .last()
                    .map(|l: &MipLevel| &l.tile_store)
                    .unwrap_or(mip0),
                &store,
                level_idx,
            )?;

            let scale = 0.5_f32.powi(level_idx as i32);
            let tile_grid = TileGrid::new(width, height, tile_size);
            levels.push(MipLevel {
                index: level_idx as usize,
                width,
                height,
                scale,
                tile_grid,
                tile_store: store,
                generated: true,
            });
            level_idx += 1;
        }

        tracing::debug!("generate_from_mip0: {} levels generated", levels.len());
        Ok(Self { levels })
    }
}

// ---------------------------------------------------------------------------
// Parallel MIP generation (rayon)
// ---------------------------------------------------------------------------

use rayon::prelude::*;

/// Downsample one level: 2×2 box filter, parallel over destination tiles.
fn downsample_level_rayon(
    src: &WorkingWriter,
    dst: &WorkingWriter,
    dst_mip: u32,
) -> Result<(), Error> {
    let _sw = crate::debug_stopwatch!("downsample_level_rayon");
    let tiles_x = dst.image_width().div_ceil(dst.tile_size());
    let tiles_y = dst.image_height().div_ceil(dst.tile_size());
    let tile_size = dst.tile_size();
    let src_w = src.image_width();
    let src_h = src.image_height();
    let src_mip = src.mip_level();

    (0..tiles_y).into_par_iter().try_for_each(|ty| {
        (0..tiles_x).try_for_each(|tx| {
            let coord = TileCoord::new(
                dst_mip,
                tx,
                ty,
                tile_size,
                dst.image_width(),
                dst.image_height(),
            );

            // PERFORMANCE CRITICAL: Pre-load the 4 source tiles needed for this destination tile.
            // By fetching these 4 tiles outside the pixel loop, we eliminate tens of millions of
            // `RwLock::write` cache contentions that would otherwise happen if we called `src.sample(x, y)`
            // for every single pixel. This reduces generation time from ~1 minute down to ~10ms!
            let mut src_tiles = vec![None; 4];
            for (i, (dy, dx)) in [(0, 0), (0, 1), (1, 0), (1, 1)].iter().enumerate() {
                let stx = 2 * tx + dx;
                let sty = 2 * ty + dy;
                let scoord = TileCoord::new(src_mip, stx, sty, tile_size, src_w, src_h);
                if stx * tile_size < src_w && sty * tile_size < src_h {
                    src_tiles[i] = src.read_tile(scoord)?;
                }
            }

            // Local helper to fetch a pixel from the 4 pre-loaded memory-resident tiles
            let get_px = |x: u32, y: u32| -> Result<Rgba<f16>, Error> {
                let stx = x / tile_size;
                let sty = y / tile_size;
                let dx = x % tile_size;
                let dy = y % tile_size;
                let t_idx = ((sty - 2 * ty) * 2 + (stx - 2 * tx)) as usize;
                if let Some(t) = &src_tiles[t_idx]
                    && dx < t.coord.width
                    && dy < t.coord.height
                {
                    return Ok(t.data[(dy * t.coord.width + dx) as usize]);
                }
                Err(Error::invalid_param(format!(
                    "Tile ({},{}) missing during downsample",
                    stx, sty
                )))
            };

            let mut data = Vec::with_capacity(coord.pixel_count());

            for dy in 0..coord.height {
                for dx in 0..coord.width {
                    let sx = coord.px + dx;
                    let sy = coord.py + dy;
                    let x0 = (sx * 2).min(src_w.saturating_sub(1));
                    let x1 = (sx * 2 + 1).min(src_w.saturating_sub(1));
                    let y0 = (sy * 2).min(src_h.saturating_sub(1));
                    let y1 = (sy * 2 + 1).min(src_h.saturating_sub(1));
                    let p00 = get_px(x0, y0)?;
                    let p10 = get_px(x1, y0)?;
                    let p01 = get_px(x0, y1)?;
                    let p11 = get_px(x1, y1)?;
                    data.push(avg4(p00, p10, p01, p11));
                }
            }

            dst.write_tile_f16(&crate::model::image::Tile::new(coord, data))
        })
    })
}

#[inline]
fn avg4(a: Rgba<f16>, b: Rgba<f16>, c: Rgba<f16>, d: Rgba<f16>) -> Rgba<f16> {
    macro_rules! avg {
        ($ch:ident) => {
            f16::from_f32(
                (a.$ch.to_f32() + b.$ch.to_f32() + c.$ch.to_f32() + d.$ch.to_f32()) * 0.25,
            )
        };
    }
    Rgba {
        r: avg!(r),
        g: avg!(g),
        b: avg!(b),
        a: avg!(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(id: &uuid::Uuid) -> PathBuf {
        std::env::temp_dir().join("pixors").join(id.to_string())
    }

    #[test]
    fn pyramid_levels_descend_to_1x1() {
        let tab_id = uuid::Uuid::new_v4();
        let p = MipPyramid::new(4148, 5531, 256, test_dir(&tab_id)).unwrap();
        let last = p.levels().last().unwrap();
        assert_eq!((last.width, last.height), (1, 1));
    }

    #[test]
    fn zoom_level_selection() {
        assert_eq!(MipPyramid::level_for_zoom(1.0), 0);
        assert_eq!(MipPyramid::level_for_zoom(0.5), 0);
        assert_eq!(MipPyramid::level_for_zoom(0.25), 2);
    }

    #[test]
    fn generate_from_mip0_test() {
        use crate::model::image::Tile;
        use crate::model::storage::WorkingWriter;

        let tab_id = uuid::Uuid::new_v4();
        let tile_size = 16;

        // Create MIP 0: 32×32, grey ramp, 4 tiles of 16×16
        let mip0 = WorkingWriter::new(test_dir(&tab_id), tile_size, 32, 32).unwrap();
        for ty in 0..2 {
            for tx in 0..2 {
                let coord = TileCoord::new(0, tx, ty, tile_size, 32, 32);
                let mut data = Vec::with_capacity(coord.pixel_count());
                for y in 0..coord.height {
                    for x in 0..coord.width {
                        let v = f16::from_f32(
                            ((coord.py + y) * 32 + (coord.px + x)) as f32 / (32.0 * 32.0),
                        );
                        data.push(Rgba::new(v, v, v, f16::ONE));
                    }
                }
                mip0.write_tile_f16(&Tile::new(coord, data)).unwrap();
            }
        }

        // Generate MIP levels
        let pyramid = MipPyramid::generate_from_mip0(&mip0, &test_dir(&tab_id)).unwrap();
        assert!(
            !pyramid.levels.is_empty(),
            "should have at least one MIP level"
        );

        // Level 1: 16×16, 1 tile
        let l1 = &pyramid.levels[0];
        assert_eq!(l1.width, 16, "level 1 width");
        assert_eq!(l1.height, 16, "level 1 height");
        assert!(l1.generated);

        // Read first pixel of MIP 1 → should be average of 4 MIP 0 pixels at (0,0), (1,0), (0,1), (1,1)
        let l1_coord = TileCoord::new(1, 0, 0, tile_size, 16, 16);
        let l1_tile = l1.tile_store.read_tile(l1_coord).unwrap().unwrap();
        let p00 = mip0.sample(0, 0).unwrap();
        let p10 = mip0.sample(1, 0).unwrap();
        let p01 = mip0.sample(0, 1).unwrap();
        let p11 = mip0.sample(1, 1).unwrap();
        let expected = avg4(p00, p10, p01, p11);
        let actual = l1_tile.data[0];
        let diff = (actual.r.to_f32() - expected.r.to_f32()).abs();
        assert!(
            diff < 0.001,
            "MIP 1 first pixel should equal avg of 4 MIP 0 pixels: diff={diff}"
        );
    }
}
