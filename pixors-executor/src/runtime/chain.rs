use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use crate::data::device::Device;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{Processor, ProcessorContext};

use super::event::PipelineEvent;
use super::runner::{ItemReceiver, ItemSender, RoutedItem, Runner};

pub struct ProgressState {
    pub done: AtomicUsize,
    pub total: usize,
    pub tx: SyncSender<PipelineEvent>,
}

pub struct ChainRunner {
    pub kernels: Vec<Box<dyn Processor>>,
    pub device: Device,
    pub progress: Option<Arc<ProgressState>>,
}

impl ChainRunner {
    pub fn new(
        kernels: Vec<Box<dyn Processor>>,
        device: Device,
        progress: Option<Arc<ProgressState>>,
    ) -> Self {
        Self {
            kernels,
            device,
            progress,
        }
    }

    fn bump_progress(progress: &Option<Arc<ProgressState>>) {
        if let Some(p) = progress {
            let done = p.done.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 128 == 0 || done >= p.total || done == 1 {
                tracing::info!("[pixors] bump_progress: done={} total={}", done, p.total);
                let _ = p.tx.try_send(PipelineEvent::Progress {
                    done,
                    total: p.total,
                });
            }
        }
    }

    fn run_item_streaming(
        kernels: &mut [Box<dyn Processor>],
        device: Device,
        kernel_idx: usize,
        port: u16,
        item: Item,
        outputs: &[(ItemSender, u16, u16)],
        progress: &Option<Arc<ProgressState>>,
    ) -> Result<(), Error> {
        if kernel_idx >= kernels.len() {
            send_to_all(outputs, vec![RoutedItem { port, payload: item }]);
            return Ok(());
        }

        let mut emit = Emitter::new();
        let p = if kernel_idx == 0 { port } else { 0 };
        let ctx = ProcessorContext {
            port: p,
            device,
            emit: &mut emit,
        };

        Self::bump_progress(progress);
        kernels[kernel_idx].process(ctx, item)?;
        let items = emit.into_items();

        for next_item in items {
            Self::run_item_streaming(
                kernels,
                device,
                kernel_idx + 1,
                next_item.port,
                next_item.payload,
                outputs,
                progress,
            )?;
        }

        Ok(())
    }

    fn run_finish_streaming(
        kernels: &mut [Box<dyn Processor>],
        device: Device,
        kernel_idx: usize,
        outputs: &[(ItemSender, u16, u16)],
        progress: &Option<Arc<ProgressState>>,
    ) -> Result<(), Error> {
        if kernel_idx >= kernels.len() {
            return Ok(());
        }

        let mut emit = Emitter::new();
        let ctx = ProcessorContext {
            port: 0,
            device,
            emit: &mut emit,
        };

        kernels[kernel_idx].finish(ctx)?;
        let items = emit.into_items();

        for next_item in items {
            Self::run_item_streaming(
                kernels,
                device,
                kernel_idx + 1,
                next_item.port,
                next_item.payload,
                outputs,
                progress,
            )?;
        }

        Self::run_finish_streaming(kernels, device, kernel_idx + 1, outputs, progress)?;

        Ok(())
    }
}

impl Runner for ChainRunner {
    fn run(
        mut self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<(ItemSender, u16, u16)>,
    ) -> Result<(), Error> {
        let progress = self.progress.clone();
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
            Self::run_item_streaming(kernels, device, 0, 0, dummy, &outputs, &progress)?;
            Self::run_finish_streaming(kernels, device, 0, &outputs, &progress)?;
        } else {
            let recv = &inputs[0];
            loop {
                match recv.recv() {
                    Ok(Some(routed)) => {
                        Self::run_item_streaming(
                            kernels,
                            device,
                            0,
                            routed.port,
                            routed.payload,
                            &outputs,
                            &progress,
                        )?;
                    }
                    Ok(None) | Err(_) => {
                        Self::run_finish_streaming(kernels, device, 0, &outputs, &progress)?;
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
