use crate::color::ColorSpace;
use crate::error::Error;
use crate::image::TileCoord;
use crate::io::ImageReader;
use crate::stream::{Frame, FrameKind, FrameMeta};
use crate::storage::writer::TileWriter;
use crate::pixel::Rgba;
use half::f16;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

/// A source that emits a stream of frames.
pub trait TileSource: Send + 'static {
    fn open(self) -> Result<mpsc::Receiver<Frame>, Error>;
}

/// Opens an image file and streams its tiles as `Frame`s through a channel.
pub struct ImageFileSource {
    pub path: PathBuf,
    pub tile_size: u32,
    pub generation: u64,
}

impl ImageFileSource {
    pub fn new(path: PathBuf, tile_size: u32, generation: u64) -> Self {
        Self { path, tile_size, generation }
    }
}

impl TileSource for ImageFileSource {
    fn open(self) -> Result<mpsc::Receiver<Frame>, Error> {
        Self::open_impl(&self.path, self.tile_size, self.generation)
    }
}

impl ImageFileSource {
    fn open_impl(path: &Path, tile_size: u32, generation: u64) -> Result<mpsc::Receiver<Frame>, Error> {
        let reader = crate::io::all_readers()
            .iter()
            .find(|r| r.can_handle(path))
            .copied()
            .ok_or_else(|| Error::unsupported_sample_type("No reader for file"))?;

        let info = reader.read_document_info(path)?;
        let path_owned = path.to_path_buf();

        let (tx, rx) = mpsc::sync_channel::<Frame>(64);

        std::thread::spawn(move || {
            let _ = Self::stream_layers(reader, &path_owned, tile_size, info.layer_count, generation, &tx);
        });

        Ok(rx)
    }

    fn stream_layers(
        reader: &'static dyn ImageReader,
        path: &Path,
        tile_size: u32,
        layer_count: usize,
        generation: u64,
        tx: &mpsc::SyncSender<Frame>,
    ) -> Result<(), Error> {
        for layer_idx in 0..layer_count {
            let meta = reader.read_layer_metadata(path, layer_idx)?;
            let w = meta.desc.width;
            let h = meta.desc.height;
            let tiles_x = (w + tile_size - 1) / tile_size;
            let tiles_y = (h + tile_size - 1) / tile_size;
            let total_tiles = tiles_x * tiles_y;

            // Channel for this layer's tiles
            let (layer_tx, layer_rx) = mpsc::channel::<Vec<u8>>();

            // Spawn the reader's stream_tiles in a separate thread
            let path_owned = path.to_path_buf();
            let writer = StreamWriterNew::new(layer_tx, meta.desc.color_space, tile_size, w, h);
            std::thread::spawn(move || {
                let _ = reader.stream_tiles(&path_owned, tile_size, &writer, layer_idx, None);
                // writer is dropped here, closing layer_tx
            });

            // Collect tiles from the writer thread and emit Frame::Tile
            let mut emitted = 0u32;
            for raw in layer_rx {
                let tx_tile = (emitted % tiles_x) as u32;
                let ty_tile = emitted / tiles_x;
                let coord = TileCoord::new(0, tx_tile, ty_tile, tile_size, w, h);

                if tx.send(Frame::new(
                    FrameMeta {
                        layer_id: layer_idx as u32,
                        mip_level: 0,
                        image_w: w,
                        image_h: h,
                        color_space: meta.desc.color_space,
                        total_tiles,
                        generation,
                    },
                    FrameKind::Tile { coord },
                    raw,
                )).is_err() { break; }
                emitted += 1;

                if emitted % 50 == 0 || emitted == total_tiles {
                    tracing::debug!("source: emitted {}/{} tiles", emitted, total_tiles);
                }
            }

            if tx.send(Frame::new(
                FrameMeta { layer_id: layer_idx as u32, mip_level: 0, image_w: w, image_h: h, color_space: meta.desc.color_space, total_tiles, generation },
                FrameKind::LayerDone,
                vec![],
            )).is_err() { break; }
        }

        let _ = tx.send(Frame::new(
            FrameMeta { layer_id: 0, mip_level: 0, image_w: 0, image_h: 0, color_space: ColorSpace::SRGB, total_tiles: 0, generation },
            FrameKind::StreamDone,
            vec![],
        ));
        Ok(())
    }
}

// ── StreamWriterNew — implements TileWriter<u8>, emits raw bytes through channel ──

struct StreamWriterNew {
    tx: mpsc::Sender<Vec<u8>>,
    #[allow(dead_code)]
    color_space: ColorSpace,
    #[allow(dead_code)]
    tile_size: u32,
    #[allow(dead_code)]
    image_width: u32,
    #[allow(dead_code)]
    image_height: u32,
}

impl StreamWriterNew {
    fn new(tx: mpsc::Sender<Vec<u8>>, color_space: ColorSpace, tile_size: u32, w: u32, h: u32) -> Self {
        Self { tx, color_space, tile_size, image_width: w, image_height: h }
    }
}

impl TileWriter<u8> for StreamWriterNew {
    fn write_tile(&self, _coord: TileCoord, pixels: &[u8]) -> Result<(), Error> {
        self.tx.send(pixels.to_vec()).ok();
        Ok(())
    }

    fn name(&self) -> &'static str { "StreamWriterNew" }
}

// ── WorkSource — skeleton for Phase 9 operations ──

/// A source that emits pre-computed tiles (e.g., from operations).
pub struct WorkSource {
    pub tiles: Vec<(TileCoord, Arc<Vec<Rgba<f16>>>)>,
    pub meta: FrameMeta,
}

impl WorkSource {
    pub fn new(meta: FrameMeta) -> Self {
        Self { tiles: Vec::new(), meta }
    }

    pub fn add_tile(&mut self, coord: TileCoord, data: Arc<Vec<Rgba<f16>>>) {
        self.tiles.push((coord, data));
    }
}

impl TileSource for WorkSource {
    fn open(self) -> Result<mpsc::Receiver<Frame>, Error> {
        let (tx, rx) = mpsc::sync_channel::<Frame>(64);
        std::thread::spawn(move || {
            for (coord, data) in self.tiles.into_iter() {
                let bytes = bytemuck::cast_slice::<Rgba<f16>, u8>(&data).to_vec();
                let frame = Frame::new(self.meta, FrameKind::Tile { coord }, bytes);
                if tx.send(frame).is_err() { break; }
            }
            let _ = tx.send(Frame::new(self.meta, FrameKind::StreamDone, vec![]));
        });
        Ok(rx)
    }
}
