use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::CpuKernel;

use super::runner::{ItemReceiver, ItemSender, Runner};

/// CPU chain runner. Holds an ordered list of CpuKernel stages that execute
/// sequentially in a single thread. Items flow through the chain via an Emitter
/// at each step; the final stage's output goes to the output channels.
///
/// Fan-out: if multiple output channels exist, emitted items are cloned to each.
pub struct CpuChainRunner {
    pub kernels: Vec<Box<dyn CpuKernel>>,
}

impl CpuChainRunner {
    pub fn new(kernels: Vec<Box<dyn CpuKernel>>) -> Self {
        Self { kernels }
    }

    fn run_item(kernels: &mut [Box<dyn CpuKernel>], item: Item) -> Result<Vec<Item>, Error> {
        let mut current = vec![item];
        for kernel in kernels.iter_mut() {
            let mut next = Vec::new();
            for item in current {
                let mut emit = Emitter::new();
                kernel.process(item, &mut emit)?;
                next.extend(emit.into_items());
            }
            current = next;
        }
        Ok(current)
    }

    fn run_finish(kernels: &mut [Box<dyn CpuKernel>]) -> Result<Vec<Item>, Error> {
        let mut all_outputs: Vec<Item> = Vec::new();
        let n = kernels.len();
        for i in 0..n {
            let mut emit = Emitter::new();
            kernels[i].finish(&mut emit)?;
            let items = emit.into_items();
            if i + 1 < n {
                for item in items {
                    let outputs = Self::run_item(&mut kernels[i + 1..], item)?;
                    all_outputs.extend(outputs);
                }
            } else {
                all_outputs.extend(items);
            }
        }
        Ok(all_outputs)
    }
}

impl Runner for CpuChainRunner {
    fn run(
        mut self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<ItemSender>,
    ) -> Result<(), Error> {
        let kernels = &mut self.kernels;

        if inputs.is_empty() {
            // Source: kick off with a dummy item, then finish.
            use crate::data::{Buffer, Tile, TileCoord};
            use crate::model::pixel::meta::PixelMeta;
            use crate::model::pixel::{AlphaPolicy, PixelFormat};
            use crate::model::color::ColorSpace;
            let dummy = Item::Tile(Tile::new(
                TileCoord::new(0, 0, 0, 0, 0, 0),
                PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight),
                Buffer::cpu(vec![]),
            ));
            let items = CpuChainRunner::run_item(kernels, dummy)?;
            send_to_all(&outputs, items);
            let finish_items = CpuChainRunner::run_finish(kernels)?;
            send_to_all(&outputs, finish_items);
        } else {
            let recv = &inputs[0];
            loop {
                match recv.recv() {
                    Ok(Some(item)) => {
                        let items = CpuChainRunner::run_item(kernels, item)?;
                        send_to_all(&outputs, items);
                    }
                    Ok(None) | Err(_) => {
                        let finish_items = CpuChainRunner::run_finish(kernels)?;
                        send_to_all(&outputs, finish_items);
                        break;
                    }
                }
            }
        }

        // Propagate EOS to all output channels.
        for out in &outputs {
            let _ = out.send(None);
        }
        Ok(())
    }
}

fn send_to_all(outputs: &[ItemSender], items: Vec<Item>) {
    if outputs.is_empty() || items.is_empty() {
        return;
    }
    for item in items {
        for out in outputs.iter() {
            let _ = out.send(Some(item.clone()));
        }
    }
}
