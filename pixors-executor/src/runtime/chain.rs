use crate::data::device::Device;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{Processor, ProcessorContext};

use super::runner::{ItemReceiver, ItemSender, RoutedItem, Runner};

pub struct ChainRunner {
    pub kernels: Vec<Box<dyn Processor>>,
    pub device: Device,
}

impl ChainRunner {
    pub fn new(kernels: Vec<Box<dyn Processor>>, device: Device) -> Self {
        Self { kernels, device }
    }

    fn run_item(
        kernels: &mut [Box<dyn Processor>],
        port: u16,
        device: Device,
        item: Item,
    ) -> Result<Vec<RoutedItem>, Error> {
        let mut current = vec![RoutedItem {
            port,
            payload: item,
        }];
        for (i, kernel) in kernels.iter_mut().enumerate() {
            let mut next = Vec::new();
            for routed_item in current {
                let mut emit = Emitter::new();
                let p = if i == 0 { routed_item.port } else { 0 };
                let ctx = ProcessorContext {
                    port: p,
                    device,
                    emit: &mut emit,
                };
                kernel.process(ctx, routed_item.payload)?;
                next.extend(emit.into_items());
            }
            current = next;
        }
        Ok(current)
    }

    fn run_finish(
        kernels: &mut [Box<dyn Processor>],
        device: Device,
    ) -> Result<Vec<RoutedItem>, Error> {
        let mut all_outputs: Vec<RoutedItem> = Vec::new();
        let n = kernels.len();
        for i in 0..n {
            let mut emit = Emitter::new();
            let ctx = ProcessorContext {
                port: 0,
                device,
                emit: &mut emit,
            };
            kernels[i].finish(ctx)?;
            let items = emit.into_items();
            if i + 1 < n {
                for routed_item in items {
                    let outputs =
                        Self::run_item(&mut kernels[i + 1..], 0, device, routed_item.payload)?;
                    all_outputs.extend(outputs);
                }
            } else {
                all_outputs.extend(items);
            }
        }
        Ok(all_outputs)
    }
}

impl Runner for ChainRunner {
    fn run(
        mut self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<(ItemSender, u16, u16)>,
    ) -> Result<(), Error> {
        let kernels = &mut self.kernels;
        let device = self.device;

        if inputs.is_empty() {
            use crate::data::buffer::Buffer;
            use crate::data::tile::{Tile, TileCoord};
            use crate::model::color::space::ColorSpace;
            use crate::model::pixel::meta::PixelMeta;
            use crate::model::pixel::{AlphaPolicy, PixelFormat};
            let dummy = Item::Tile(Tile::new(
                TileCoord::new(0, 0, 0, 0, 0, 0),
                PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight),
                Buffer::cpu(vec![]),
            ));
            let items = ChainRunner::run_item(kernels, 0, device, dummy)?;
            send_to_all(&outputs, items);
            let finish_items = ChainRunner::run_finish(kernels, device)?;
            send_to_all(&outputs, finish_items);
        } else {
            let recv = &inputs[0];
            loop {
                match recv.recv() {
                    Ok(Some(routed)) => {
                        let items =
                            ChainRunner::run_item(kernels, routed.port, device, routed.payload)?;
                        send_to_all(&outputs, items);
                    }
                    Ok(None) | Err(_) => {
                        let finish_items = ChainRunner::run_finish(kernels, device)?;
                        send_to_all(&outputs, finish_items);
                        break;
                    }
                }
            }
        }

        for (tx, _, _) in &outputs {
            let _ = tx.send(None);
        }
        Ok(())
    }
}

fn send_to_all(outputs: &[(ItemSender, u16, u16)], items: Vec<RoutedItem>) {
    if outputs.is_empty() || items.is_empty() {
        return;
    }
    for routed_item in items {
        for (tx, from_port, to_port) in outputs.iter() {
            if routed_item.port == *from_port {
                let routed = RoutedItem {
                    port: *to_port,
                    payload: routed_item.payload.clone(),
                };
                let _ = tx.send(Some(routed));
            }
        }
    }
}
