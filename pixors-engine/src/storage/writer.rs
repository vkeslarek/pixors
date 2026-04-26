//! Tile-level writers — destinations for tile data.
//!
//! - `WorkingWriter`: disk-backed tile storage (ACEScg f16) — owns the tiles,
//!    converts raw bytes → ACEScg f16, reads/writes/caches tiles on disk.
//! - `DisplayWriter`: raw bytes → sRGB u8 → full RAM HashMap
//! - `FanoutWriter`: sync DisplayWriter + async channel to background disk thread

use crate::convert::ColorConversion;
use crate::error::Error;
use crate::image::buffer::BufferDesc;
use crate::image::{Tile, TileCoord};
use crate::pixel::{AlphaPolicy, Rgba};
use bytemuck::Pod;
use half::f16;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use uuid::Uuid;

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

// ---------------------------------------------------------------------------
// DisplayWriter — raw bytes (source) → sRGB u8 → full RAM HashMap
// ---------------------------------------------------------------------------

pub struct DisplayWriter {
    conv: ColorConversion,
    desc: BufferDesc,
    tiles: RwLock<HashMap<(Uuid, u32, TileCoord), Arc<Vec<u8>>>>,
    layer_id: Uuid,
    mip_level: u32,
    generated_levels: AtomicU32,
}

impl DisplayWriter {
    pub fn new(conv: ColorConversion, desc: BufferDesc, layer_id: Uuid) -> Self {
        Self {
            conv,
            desc,
            tiles: RwLock::new(HashMap::new()),
            layer_id,
            mip_level: 0,
            generated_levels: AtomicU32::new(1),
        }
    }

    pub fn set_mip_level(&mut self, mip: u32) {
        self.mip_level = mip;
    }

    pub fn get(&self, mip: u32, coord: TileCoord) -> Option<Arc<Vec<u8>>> {
        self.tiles
            .read()
            .get(&(self.layer_id, mip, coord))
            .cloned()
    }

    pub fn put(&self, mip: u32, coord: TileCoord, data: Arc<Vec<u8>>) {
        self.tiles
            .write()
            .insert((self.layer_id, mip, coord), data);
    }

    pub fn mark_level_generated(&self, mip: u32) {
        self.generated_levels.fetch_or(1 << mip, Ordering::Release);
    }

    pub fn is_level_generated(&self, mip: u32) -> bool {
        self.generated_levels.load(Ordering::Acquire) & (1 << mip) != 0
    }
}

impl TileWriter<u8> for DisplayWriter {
    fn write_tile(&self, coord: TileCoord, pixels: &[u8]) -> Result<(), Error> {
        let bpp = self.desc.planes.len() * self.desc.planes[0].encoding.byte_size();
        let actual_pixels = pixels.len() / bpp.max(1);
        let tile_w = coord.width as usize;
        let tile_h = actual_pixels / tile_w.max(1);
        let tile_stride = tile_w * bpp;
        let mut desc = self.desc.clone();
        desc.width = tile_w as u32;
        desc.height = tile_h as u32;
        for p in &mut desc.planes {
            p.row_length = tile_w as u32;
            p.row_stride = tile_stride;
        }
        let srgb: Vec<[u8; 4]> = self
            .conv
            .convert_buffer(pixels, &desc, AlphaPolicy::Straight);
        let bytes: Vec<u8> = bytemuck::cast_slice::<[u8; 4], u8>(&srgb).to_vec();
        self.tiles
            .write()
            .insert((self.layer_id, self.mip_level, coord), Arc::new(bytes));
        Ok(())
    }

    fn name(&self) -> &'static str {
        "DisplayWriter"
    }
}

// ---------------------------------------------------------------------------
// FanoutWriter — sync DisplayWriter + async channel to disk thread
// ---------------------------------------------------------------------------

pub struct FanoutWriter<'a> {
    display: &'a DisplayWriter,
    disk_tx: mpsc::Sender<(TileCoord, Vec<u8>)>,
}

impl<'a> FanoutWriter<'a> {
    pub fn new(display: &'a DisplayWriter, disk_tx: mpsc::Sender<(TileCoord, Vec<u8>)>) -> Self {
        Self { display, disk_tx }
    }
}

impl<'a> TileWriter<u8> for FanoutWriter<'a> {
    fn write_tile(&self, coord: TileCoord, pixels: &[u8]) -> Result<(), Error> {
        self.display.write_tile(coord, pixels)?;
        let _ = self.disk_tx.send((coord, pixels.to_vec()));
        Ok(())
    }

    fn name(&self) -> &'static str {
        "FanoutWriter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::buffer::{BufferDesc, PlaneDesc, SampleFormat};
    use crate::image::AlphaMode;

    fn test_dir(id: &Uuid) -> PathBuf {
        std::env::temp_dir().join("pixors").join(id.to_string())
    }

    #[test]
    fn fanout_writes_to_display_and_channel() {
        let desc = BufferDesc {
            width: 16,
            height: 16,
            planes: vec![
                PlaneDesc { offset: 0, stride: 4, row_stride: 64, row_length: 16, encoding: SampleFormat::U8 },
                PlaneDesc { offset: 1, stride: 4, row_stride: 64, row_length: 16, encoding: SampleFormat::U8 },
                PlaneDesc { offset: 2, stride: 4, row_stride: 64, row_length: 16, encoding: SampleFormat::U8 },
                PlaneDesc { offset: 3, stride: 4, row_stride: 64, row_length: 16, encoding: SampleFormat::U8 },
            ],
            color_space: crate::color::ColorSpace::SRGB,
            alpha_mode: AlphaMode::Straight,
        };
        let conv = crate::color::ColorSpace::SRGB
            .converter_to(crate::color::ColorSpace::SRGB)
            .unwrap();
        let layer_id = Uuid::new_v4();
        let display = DisplayWriter::new(conv, desc, layer_id);
        let (tx, rx) = mpsc::channel();

        let fanout = FanoutWriter::new(&display, tx);
        let coord = TileCoord::new(0, 0, 0, 16, 16, 16);
        let pixels = vec![128u8; 16 * 16 * 4];
        fanout.write_tile(coord, &pixels).unwrap();

        // Display should have the tile
        assert!(display.get(0, coord).is_some());

        // Channel should have received the tile
        let (rx_coord, rx_data) = rx.recv().unwrap();
        assert_eq!(rx_coord, coord);
        assert_eq!(rx_data.len(), pixels.len());
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
