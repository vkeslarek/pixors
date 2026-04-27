//! Tile-level writers — destinations for tile data.
//!
//! - `WorkingWriter`: disk-backed tile storage (ACEScg f16) — owns the tiles,
//!  converts raw bytes → ACEScg f16, reads/writes/caches tiles on disk.

use crate::error::Error;
use crate::image::{Tile, TileCoord};
use crate::pixel::Rgba;
use bytemuck::Pod;
use half::f16;
use std::path::PathBuf;
use std::sync::Arc;

pub trait TileWriter<P: Pod>: Sync {
    fn write_tile(&self, coord: TileCoord, pixels: &[P]) -> Result<(), Error>;
    fn finish(&self) -> Result<(), Error> {
        Ok(())
    }
    fn name(&self) -> &'static str;
}

// ═══════════════════════════════════════════════════════════════════════════
// WorkingWriter — full disk-backed tile store (ACEScg f16 read/write/cache)
// ═══════════════════════════════════════════════════════════════════════════

/// Disk-backed tile storage for ACEScg f16 tiles.
/// Each tile file: `{base_dir}/tile_{mip}_{tx}_{ty}.raw`
/// When constructed for writing, holds color conversion to ingest raw source bytes.
#[derive(Debug)]
pub struct WorkingWriter {
    base_dir: PathBuf,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    mip_level: u32,
    auto_destroy: bool,
}

impl WorkingWriter {
    pub fn new(
        base_dir: PathBuf, tile_size: u32, image_width: u32, image_height: u32,
    ) -> Result<Self, Error> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir, tile_size, image_width, image_height, mip_level: 0, auto_destroy: true })
    }

    pub fn new_with_subdir(
        base_dir: PathBuf, subdir: &str, tile_size: u32, image_width: u32, image_height: u32,
    ) -> Result<Self, Error> {
        let base_dir = base_dir.join(subdir);
        std::fs::create_dir_all(&base_dir)?;
        let mip_level = subdir.strip_prefix("mip_").and_then(|s| s.parse().ok()).unwrap_or(0);
        Ok(Self { base_dir, tile_size, image_width, image_height, mip_level, auto_destroy: true })
    }

    pub fn open(
        base_dir: PathBuf, tile_size: u32, image_width: u32, image_height: u32,
    ) -> Result<Self, Error> {
        Ok(Self { base_dir, tile_size, image_width, image_height, mip_level: 0, auto_destroy: false })
    }
    // -- accessors ----------------------------------------------------------

    pub fn base_dir(&self) -> PathBuf { self.base_dir.clone() }
    pub fn tile_size(&self) -> u32 { self.tile_size }
    pub fn image_width(&self) -> u32 { self.image_width }
    pub fn image_height(&self) -> u32 { self.image_height }
    pub fn mip_level(&self) -> u32 { self.mip_level }

    // -- internal helpers ---------------------------------------------------

    fn tile_path(&self, tile: &TileCoord) -> PathBuf {
        self.base_dir
            .join(format!("tile_{}_{}_{}.raw", tile.mip_level, tile.tx, tile.ty))
    }

    fn serialize_le(data: &[Rgba<f16>]) -> Vec<u8> {
        #[cfg(target_endian = "little")]
        { bytemuck::cast_slice::<Rgba<f16>, u8>(data).to_vec() }
        #[cfg(not(target_endian = "little"))]
        {
            let mut out = Vec::with_capacity(data.len() * 8);
            for px in data {
                out.extend_from_slice(&px.r.to_le_bytes());
                out.extend_from_slice(&px.g.to_le_bytes());
                out.extend_from_slice(&px.b.to_le_bytes());
                out.extend_from_slice(&px.a.to_le_bytes());
            }
            out
        }
    }

    fn deserialize_le(bytes: &[u8]) -> Vec<Rgba<f16>> {
        #[cfg(target_endian = "little")]
        { bytemuck::cast_slice::<u8, Rgba<f16>>(bytes).to_vec() }
        #[cfg(not(target_endian = "little"))]
        {
            bytes.chunks_exact(8).map(|c| Rgba {
                r: f16::from_le_bytes([c[0], c[1]]),
                g: f16::from_le_bytes([c[2], c[3]]),
                b: f16::from_le_bytes([c[4], c[5]]),
                a: f16::from_le_bytes([c[6], c[7]]),
            }).collect()
        }
    }

    // -- disk I/O -----------------------------------------------------------

    pub fn read_tile(&self, coord: TileCoord) -> Result<Option<Tile<Rgba<f16>>>, Error> {
        let path = self.tile_path(&coord);
        if !path.exists() { return Ok(None); }
        let bytes = std::fs::read(&path).map_err(Error::Io)?;
        if bytes.len() != coord.pixel_count() * 8 {
            return Err(Error::invalid_param("Tile file size mismatch"));
        }
        let pixels = Self::deserialize_le(&bytes);
        Ok(Some(Tile { coord, data: Arc::new(pixels) }))
    }

    pub fn write_tile_f16(&self, tile: &Tile<Rgba<f16>>) -> Result<(), Error> {
        let path = self.tile_path(&tile.coord);
        std::fs::write(&path, Self::serialize_le(&tile.data)).map_err(Error::Io)
    }

    pub fn sample(&self, x: u32, y: u32) -> Result<Rgba<f16>, Error> {
        if x >= self.image_width || y >= self.image_height {
            return Err(Error::invalid_param(format!(
                "sample ({},{}) out of bounds ({}x{})", x, y, self.image_width, self.image_height
            )));
        }
        let tx = x / self.tile_size;
        let ty = y / self.tile_size;
        let coord = TileCoord::new(self.mip_level, tx, ty, self.tile_size, self.image_width, self.image_height);
        let tile = self.read_tile(coord)?.ok_or_else(|| {
            Error::invalid_param(format!("Tile ({},{}) not stored", tx, ty))
        })?;
        let lx = x - tile.coord.px;
        let ly = y - tile.coord.py;
        Ok(tile.data[(ly * tile.coord.width + lx) as usize])
    }

    pub fn has(&self, tile: &TileCoord) -> bool { self.tile_path(tile).exists() }

    pub fn destroy(&self) -> Result<(), Error> {
        if self.base_dir.exists() {
            std::fs::remove_dir_all(&self.base_dir).map_err(Error::Io)
        } else {
            Ok(())
        }
    }
}

impl Drop for WorkingWriter {
    fn drop(&mut self) {
        if self.auto_destroy {
            let _ = self.destroy();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_dir(id: &Uuid) -> PathBuf {
        std::env::temp_dir().join("pixors").join(id.to_string())
    }

#[test]
    fn test_write_read_roundtrip() {
        let id = Uuid::new_v4();
        let store = WorkingWriter::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = TileCoord::new(0, 0, 0, 256, 512, 512);
        let pixels: Vec<Rgba<f16>> = (0..256 * 256)
            .map(|i| {
                let v = f16::from_f32((i % 256) as f32 / 255.0);
                Rgba::new(v, v, v, f16::ONE)
            })
            .collect();
        let tile = Tile::new(coord, pixels.clone());
        store.write_tile_f16(&tile).unwrap();
        let read_back = store.read_tile(coord).unwrap().unwrap();
        assert_eq!(read_back.data.len(), pixels.len());
        assert_eq!(read_back.data[0].r, pixels[0].r);
    }

    #[test]
    fn test_read_nonexistent() {
        let id = Uuid::new_v4();
        let store = WorkingWriter::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = TileCoord::new(0, 99, 99, 256, 512, 512);
        assert!(store.read_tile(coord).unwrap().is_none());
    }

    #[test]
    fn test_sample() {
        let id = Uuid::new_v4();
        let store = WorkingWriter::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = TileCoord::new(0, 0, 0, 256, 512, 512);
        let pixels = vec![Rgba::new(f16::from_f32(0.5), f16::from_f32(0.3), f16::from_f32(0.2), f16::ONE); 256 * 256];
        let tile = Tile::new(coord, pixels);
        store.write_tile_f16(&tile).unwrap();
        let px = store.sample(10, 10).unwrap();
        assert!((px.r.to_f32() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_sample_out_of_bounds() {
        let id = Uuid::new_v4();
        let store = WorkingWriter::new(test_dir(&id), 256, 512, 512).unwrap();
        assert!(store.sample(600, 600).is_err());
    }
}
