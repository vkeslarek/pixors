use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixors_engine::pipeline::exec::tile_sink::install_tile_sink;
use pixors_engine::pipeline::state::{Blur, DisplayCache, FileImage, StateNode};
use pixors_engine::pipeline::state_graph::builder::PathBuilder;
use pixors_engine::pipeline::state_graph::compile::ExecutionMode;

use crate::viewport::program::{PendingTile, PendingTileWrites};

/// Open a file dialog, load image, run pipeline with 2×blur, enqueue tiles.
pub fn open_and_run(pending: &Arc<PendingTileWrites>) -> Result<(u32, u32, PathBuf), String> {
    let path = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "tiff", "tif"])
        .pick_file()
        .ok_or_else(|| "cancelled".to_string())?;

    let (w, h) = probe_dimensions(&path)?;

    // Install tile sink → pushes tiles to pending_writes for the GPU render thread
    let p = pending.clone();
    install_tile_sink(Box::new(move |px, py, tw, th, bytes| {
        let mut q = p.queue.lock().unwrap();
        q.push(PendingTile {
            px,
            py,
            tile_w: tw,
            tile_h: th,
            bytes: bytes.to_vec(),
        });
    }));

    PathBuilder::new()
        .source(StateNode::FileImage(FileImage {
            path: path.clone(),
        }))
        .operation(StateNode::Blur(Blur { radius: 8 }))
        .operation(StateNode::Blur(Blur { radius: 8 }))
        .sink(StateNode::DisplayCache(DisplayCache { generation: 0 }))
        .run(ExecutionMode::Apply { force_cpu: true })
        .map_err(|e| format!("Pipeline error: {e:?}"))?;

    Ok((w, h, path))
}

fn probe_dimensions(path: &Path) -> Result<(u32, u32), String> {
    use pixors_engine::io;
    let readers = io::all_readers();
    for r in readers {
        if r.can_handle(path) {
            let info = r
                .read_document_info(path)
                .map_err(|e| format!("{e:?}"))?;
            let meta = r
                .read_layer_metadata(path, 0)
                .map_err(|e| format!("{e:?}"))?;
            return Ok((meta.desc.width, meta.desc.height));
        }
    }
    Err("No reader".into())
}
