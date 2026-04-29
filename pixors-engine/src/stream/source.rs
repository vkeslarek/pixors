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
            let tiles_x = w.div_ceil(tile_size);
            let tiles_y = h.div_ceil(tile_size);
            let total_tiles = tiles_x * tiles_y;

            let (layer_tx, layer_rx) = mpsc::channel::<Vec<u8>>();
            let path_owned = path.to_path_buf();
            struct RawWriter(mpsc::Sender<Vec<u8>>);
            impl TileWriter<u8> for RawWriter {
                fn write_tile(&self, _coord: TileCoord, pixels: &[u8]) -> Result<(), Error> { self.0.send(pixels.to_vec()).ok(); Ok(()) }
                fn name(&self) -> &'static str { "RawWriter" }
            }
            std::thread::spawn(move || {
                let _ = reader.stream_tiles(&path_owned, tile_size, &RawWriter(layer_tx), layer_idx, None);
            });

            let mut emitted = 0u32;
            for raw in layer_rx {
                let tx_tile = emitted % tiles_x;
                let ty_tile = emitted / tiles_x;
                let coord = TileCoord::new(0, tx_tile, ty_tile, tile_size, w, h);

                if tx.send(Frame::new(
                    FrameMeta { layer_id: layer_idx as u32, mip_level: 0, image_w: w, image_h: h, color_space: meta.desc.color_space, total_tiles, generation },
                    FrameKind::Tile { coord }, raw,
                )).is_err() { break; }
                emitted += 1;

                if emitted.is_multiple_of(10) || emitted == total_tiles {
                    let _ = tx.send(Frame::new(
                        FrameMeta { layer_id: layer_idx as u32, mip_level: 0, image_w: w, image_h: h, color_space: meta.desc.color_space, total_tiles, generation },
                        FrameKind::Progress { done: emitted, total: total_tiles }, vec![],
                    ));
                }
            }

            if tx.send(Frame::new(
                FrameMeta { layer_id: layer_idx as u32, mip_level: 0, image_w: w, image_h: h, color_space: meta.desc.color_space, total_tiles, generation },
                FrameKind::LayerDone, vec![],
            )).is_err() { break; }
        }

        let _ = tx.send(Frame::new(
            FrameMeta { layer_id: 0, mip_level: 0, image_w: 0, image_h: 0, color_space: ColorSpace::SRGB, total_tiles: 0, generation },
            FrameKind::StreamDone, vec![],
        ));
        Ok(())
    }
}

