use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::time::Duration;

use crate::data::device::Device;
use crate::error::Error;
use crate::gpu::context::GpuContext;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{Consumer, Processor, ProcessorContext, Producer};

use super::event::PipelineEvent;
use super::runner::{ItemReceiver, ItemSender, RoutedItem, Runner};

pub struct ProgressState {
    pub done: AtomicUsize,
    pub total: usize,
    pub tx: SyncSender<PipelineEvent>,
}

pub struct ChainRunner {
    pub producer: Option<Box<dyn Producer>>,
    pub kernels: Vec<Box<dyn Processor>>,
    pub consumer: Option<Box<dyn Consumer>>,
    pub device: Device,
    pub gpu: Option<Arc<GpuContext>>,
    pub progress: Option<Arc<ProgressState>>,
    pub chain_name: String,
    pub cancelled: Arc<AtomicBool>,
    pub tag: u64,
}

impl ChainRunner {
    pub fn new(
        producer: Option<Box<dyn Producer>>,
        kernels: Vec<Box<dyn Processor>>,
        consumer: Option<Box<dyn Consumer>>,
        device: Device,
        gpu: Option<Arc<GpuContext>>,
        progress: Option<Arc<ProgressState>>,
        chain_name: String,
        cancelled: Arc<AtomicBool>,
        tag: u64,
    ) -> Self {
        Self {
            producer,
            kernels,
            consumer,
            device,
            gpu,
            progress,
            chain_name,
            cancelled,
            tag,
        }
    }

    fn bump_progress(tag: u64, progress: &Option<Arc<ProgressState>>) {
        if let Some(p) = progress {
            let done = p.done.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 128 == 0 || done >= p.total || done == 1 {
                let _ = p.tx.try_send(PipelineEvent::Progress {
                    tag,
                    done,
                    total: p.total,
                });
            }
        }
    }

    fn run_item_streaming(
        cancelled: &Arc<AtomicBool>,
        tag: u64,
        kernels: &mut [Box<dyn Processor>],
        consumer: &mut Option<Box<dyn Consumer>>,
        device: Device,
        gpu: &Option<Arc<GpuContext>>,
        kernel_idx: usize,
        port: u16,
        item: Item,
        outputs: &[(ItemSender, u16, u16)],
        progress: &Option<Arc<ProgressState>>,
    ) -> Result<(), Error> {
        if kernel_idx >= kernels.len() {
            match consumer {
                Some(c) => {
                    c.consume(item)?;
                    Self::bump_progress(tag, progress);
                }
                None => send_to_all(
                    outputs,
                    vec![RoutedItem {
                        port,
                        payload: item,
                    }],
                    gpu,
                ),
            }
            return Ok(());
        }

        let mut emit = Emitter::new();
        let p = if kernel_idx == 0 { port } else { 0 };
        let ctx = ProcessorContext {
            port: p,
            device,
            emit: &mut emit,
            gpu: gpu.clone(),
            cancelled: cancelled.clone(),
        };
        Self::bump_progress(tag, progress);
        kernels[kernel_idx].process(ctx, item)?;
        let items = emit.into_items();
        for next_item in items {
            Self::run_item_streaming(
                cancelled,
                tag,
                kernels,
                consumer,
                device,
                gpu,
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
        cancelled: &Arc<AtomicBool>,
        tag: u64,
        kernels: &mut [Box<dyn Processor>],
        consumer: &mut Option<Box<dyn Consumer>>,
        device: Device,
        gpu: &Option<Arc<GpuContext>>,
        kernel_idx: usize,
        outputs: &[(ItemSender, u16, u16)],
        progress: &Option<Arc<ProgressState>>,
    ) -> Result<(), Error> {
        if kernel_idx >= kernels.len() {
            if let Some(c) = consumer {
                c.finish()?;
            }
            return Ok(());
        }
        let mut emit = Emitter::new();
        let ctx = ProcessorContext {
            port: 0,
            device,
            emit: &mut emit,
            gpu: gpu.clone(),
            cancelled: cancelled.clone(),
        };
        kernels[kernel_idx].finish(ctx)?;
        let items = emit.into_items();
        for next_item in items {
            Self::run_item_streaming(
                cancelled,
                tag,
                kernels,
                consumer,
                device,
                gpu,
                kernel_idx + 1,
                next_item.port,
                next_item.payload,
                outputs,
                progress,
            )?;
        }
        Self::run_finish_streaming(
            cancelled,
            tag,
            kernels,
            consumer,
            device,
            gpu,
            kernel_idx + 1,
            outputs,
            progress,
        )?;
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
        let gpu = &self.gpu;
        let name = self.chain_name.clone();
        let num_outputs = outputs.len();
        let tag = self.tag;

        if let Some(mut producer) = self.producer.take() {
            let mut emit = Emitter::new();
            let ctx = ProcessorContext {
                port: 0,
                device,
                gpu: gpu.clone(),
                cancelled: self.cancelled.clone(),
                emit: &mut emit,
            };
            producer.produce(ctx)?;
            let items = emit.into_items();
            for item in items {
                Self::run_item_streaming(
                    &self.cancelled,
                    tag,
                    kernels,
                    &mut self.consumer,
                    device,
                    gpu,
                    0,
                    item.port,
                    item.payload,
                    &outputs,
                    &progress,
                )?;
            }
            Self::run_finish_streaming(
                &self.cancelled,
                tag,
                kernels,
                &mut self.consumer,
                device,
                gpu,
                0,
                &outputs,
                &progress,
            )?;
        } else if inputs.is_empty() {
            Self::run_finish_streaming(
                &self.cancelled,
                tag,
                kernels,
                &mut self.consumer,
                device,
                gpu,
                0,
                &outputs,
                &progress,
            )?;
        } else {
            let recv = &inputs[0];
            let mut item_count = 0u64;
            loop {
                if self.cancelled.load(Ordering::Relaxed) {
                    tracing::debug!("[pixors] {name}: cancelled after {item_count} items");
                    return Ok(());
                }
                match recv.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(routed)) => {
                        Self::run_item_streaming(
                            &self.cancelled,
                            tag,
                            kernels,
                            &mut self.consumer,
                            device,
                            gpu,
                            0,
                            routed.port,
                            routed.payload,
                            &outputs,
                            &progress,
                        )?;
                        item_count += 1;
                    }
                    Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        tracing::debug!(
                            "[pixors] {name}: received None after {item_count} items, entering finish phase…"
                        );
                        Self::run_finish_streaming(
                            &self.cancelled,
                            tag,
                            kernels,
                            &mut self.consumer,
                            device,
                            gpu,
                            0,
                            &outputs,
                            &progress,
                        )?;
                        tracing::debug!(
                            "[pixors] {name}: finish phase complete, signalling Done to {num_outputs} output(s)"
                        );
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        continue;
                    }
                }
            }
        }

        for (tx, _, _) in &outputs {
            let _ = tx.send(None);
        }
        tracing::debug!("[pixors] {name}: RUN COMPLETE");
        Ok(())
    }
}

fn send_to_all(
    outputs: &[(ItemSender, u16, u16)],
    items: Vec<RoutedItem>,
    gpu: &Option<Arc<GpuContext>>,
) {
    if outputs.is_empty() || items.is_empty() {
        return;
    }
    if let Some(gpu_ctx) = gpu {
        gpu_ctx.scheduler().flush_dispatches();
    }
    for routed_item in items {
        for (tx, from_port, to_port) in outputs.iter() {
            if routed_item.port == *from_port {
                let routed = RoutedItem {
                    port: *to_port,
                    payload: routed_item.payload.clone(),
                };
                if let Err(std::sync::mpsc::TrySendError::Full(item)) = tx.try_send(Some(routed)) {
                    let _ = tx.send(item);
                }
            }
        }
    }
}
