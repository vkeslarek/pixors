use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, mpsc::sync_channel};
use std::thread::{self, JoinHandle};

use crate::data::device::Device;
use crate::error::Error;
use crate::gpu;
use crate::graph::graph::{EdgePorts, ExecGraph, StageId};
use crate::operation::transfer::download::Download;
use crate::operation::transfer::upload::Upload;
use crate::stage::{Producer, Processor, Consumer, Stage, PortGroup};

use super::chain::ChainRunner;
use super::event::PipelineEvent;
use super::runner::{CHANNEL_BOUND, ItemReceiver, ItemSender, RoutedItem, Runner};

pub struct Pipeline {
    chains: Vec<(
        Box<dyn Runner>,
        Vec<ItemReceiver>,
        Vec<(ItemSender, u16, u16)>,
    )>,
    cancelled: Arc<AtomicBool>,
}

// ── Compile ─────────────────────────────────────────────────────────────────

impl Pipeline {
    pub fn compile(
        mut g: ExecGraph,
        progress_tx: Option<SyncSender<PipelineEvent>>,
        cancelled: Arc<AtomicBool>,
        tag: u64,
    ) -> Result<Self, Error> {
        let gpu_ok = gpu::context::gpu_available();
        let gpu_ctx = if gpu_ok { gpu::context::try_init() } else { None };

        validate_ports(&g)?;
        let mut devs = assign_devices(&g, gpu_ok);
        insert_transfers(&mut g, &mut devs);
        let order = g.toposort().map_err(Error::internal)?;
        let (chains, node_chain) = detect_chains(&g, &devs, &order);
        let (mut ch_in, mut ch_out) = build_channels(&g, &order, &node_chain, chains.len());
        let total_work = compute_work_total(&g, &order);

        let progress = match progress_tx {
            Some(ref tx) if total_work > 0 => Some(Arc::new(super::chain::ProgressState {
                done: AtomicUsize::new(0),
                total: total_work,
                tx: tx.clone(),
            })),
            _ => None,
        };

        let mut compiled = Vec::new();
        for (idx, nodes) in chains.iter().enumerate() {
            let dev = devs[&nodes[0]];
            let inputs = std::mem::take(&mut ch_in[idx]);
            let outputs = std::mem::take(&mut ch_out[idx]);
            let num_stages = nodes.len();
            let kinds: Vec<_> = nodes.iter().map(|&id| g.node_kind(id)).collect();
            let chain_name = format!("#{idx} [{dev:?}]  {}", kinds.join(" → "));
            tracing::debug!("[pixors] compile: {chain_name}  ({} stage{})", num_stages, if num_stages == 1 { "" } else { "s" });

            let has_producer = nodes.first().is_some_and(|&id| matches!(g.stage(id), Stage::Producer(_)));
            let has_consumer = nodes.last().is_some_and(|&id| matches!(g.stage(id), Stage::Consumer(_)));

            let producer: Option<Box<dyn Producer>> = if has_producer {
                match g.take_stage(nodes[0]) { Stage::Producer(p) => Some(p), _ => unreachable!() }
            } else { None };
            let consumer: Option<Box<dyn Consumer>> = if has_consumer {
                match g.take_stage(*nodes.last().unwrap()) { Stage::Consumer(c) => Some(c), _ => unreachable!() }
            } else { None };

            let mid_start = if has_producer { 1 } else { 0 };
            let mid_end = nodes.len() - if has_consumer { 1 } else { 0 };
            let processors: Vec<Box<dyn Processor>> = nodes[mid_start..mid_end]
                .iter()
                .map(|&id| match g.take_stage(id) {
                    Stage::Processor(p) => Ok(p),
                    _ => Err(Error::internal(format!("'{}': expected Processor", g.node_kind(id)))),
                })
                .collect::<Result<_, _>>()?;

            compiled.push((
                Box::new(ChainRunner::new(producer, processors, consumer, dev, gpu_ctx.clone(), progress.clone(), chain_name, cancelled.clone(), tag)) as Box<dyn Runner>,
                inputs,
                outputs,
            ));
        }

        tracing::debug!("[pixors] compile: {} chains built, total_work={total_work}", compiled.len());
        Ok(Pipeline { chains: compiled, cancelled })
    }
}

// ── Run ─────────────────────────────────────────────────────────────────────

pub struct PipelineHandle {
    cancelled: Arc<AtomicBool>,
    handles: Vec<JoinHandle<Result<(), Error>>>,
}

impl PipelineHandle {
    pub fn cancel(&self) { self.cancelled.store(true, Ordering::Relaxed); }
    pub fn is_running(&self) -> bool { self.handles.iter().any(|h| !h.is_finished()) }
    pub fn join(self) -> Result<(), Error> {
        let mut first_err: Option<Error> = None;
        for (i, h) in self.handles.into_iter().enumerate() {
            match h.join() {
                Ok(Ok(())) => tracing::debug!("[pixors] pipeline: thread {i} joined OK"),
                Ok(Err(e)) => { tracing::error!("[pixors] pipeline: thread {i} returned error: {e}"); first_err.get_or_insert(e); }
                Err(_) => { tracing::error!("[pixors] pipeline: thread {i} PANICKED"); first_err.get_or_insert(Error::internal("pipeline thread panicked")); }
            }
        }
        first_err.map(Err).unwrap_or(Ok(()))
    }
}

impl Pipeline {
    pub fn run(self, events: Option<SyncSender<PipelineEvent>>) -> PipelineHandle {
        let cancelled = self.cancelled;
        let mut handles = Vec::new();
        for (runner, inputs, outputs) in self.chains {
            let cancelled_clone = cancelled.clone();
            let merged = merge_inputs(inputs);
            let events_clone = events.clone();
            let h = thread::spawn(move || {
                let result = runner.run(merged, outputs);
                if let Err(ref e) = result && let Some(ref tx) = events_clone { let _ = tx.send(PipelineEvent::Error { tag: 0, message: e.to_string() }); }
                if cancelled_clone.load(Ordering::Relaxed) && let Some(ref tx) = events_clone { let _ = tx.send(PipelineEvent::Cancelled { tag: 0 }); }
                result
            });
            handles.push(h);
        }
        PipelineHandle { cancelled, handles }
    }
}

fn merge_inputs(inputs: Vec<ItemReceiver>) -> Vec<ItemReceiver> {
    if inputs.len() <= 1 { return inputs; }
    let n = inputs.len();
    let (tx, rx) = sync_channel::<Option<RoutedItem>>(CHANNEL_BOUND);
    let remaining = Arc::new(AtomicUsize::new(n));
    for recv in inputs {
        let tx = tx.clone();
        let remaining = remaining.clone();
        thread::spawn(move || {
            loop {
                match recv.recv() {
                    Ok(Some(routed)) => { if tx.send(Some(routed)).is_err() { break; } }
                    _ => { if remaining.fetch_sub(1, Ordering::AcqRel) == 1 { let _ = tx.send(None); } break; }
                }
            }
        });
    }
    vec![rx]
}

// ── Port validation ─────────────────────────────────────────────────────────

fn validate_ports(g: &ExecGraph) -> Result<(), Error> {
    for ei in g.edge_indices() {
        let Some(e) = g.edges.get(ei) else { continue };
        let src = e.from;
        let dst = e.to;
        let ports = e.ports;
        let src_outs = g.stage(src).output_ports();
        let dst_ins = g.stage(dst).input_ports();

        let src_output_count = outport_count(g, src, &src_outs);
        if ports.from_port as usize >= src_output_count {
            return Err(Error::internal(format!("edge {} -> {}: from_port {} out of bounds ({} output ports)", g.node_kind(src), g.node_kind(dst), ports.from_port, src_output_count)));
        }
        let dst_input_count = inport_count(g, dst, &dst_ins);
        if ports.to_port as usize >= dst_input_count {
            return Err(Error::internal(format!("edge {} -> {}: to_port {} out of bounds ({} input ports)", g.node_kind(src), g.node_kind(dst), ports.to_port, dst_input_count)));
        }
        let src_kind = src_outs.kind_at(ports.from_port as usize);
        let dst_kind = dst_ins.kind_at(ports.to_port as usize);
        if let (Some(sk), Some(dk)) = (src_kind, dst_kind) && sk != dk {
            return Err(Error::internal(format!("edge {} -> {}: DataKind mismatch (output {:?}, input {:?})", g.node_kind(src), g.node_kind(dst), sk, dk)));
        }
    }
    Ok(())
}

fn outport_count(g: &ExecGraph, id: StageId, group: &PortGroup) -> usize {
    match group {
        PortGroup::Fixed(ports) => ports.len(),
        PortGroup::Variable(_) => g.edges_out(id).map(|e| e.ports.from_port as usize + 1).max().unwrap_or(0),
    }
}

fn inport_count(g: &ExecGraph, id: StageId, group: &PortGroup) -> usize {
    match group {
        PortGroup::Fixed(ports) => ports.len(),
        PortGroup::Variable(_) => g.edges_in(id).map(|e| e.ports.to_port as usize + 1).max().unwrap_or(0),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn assign_devices(g: &ExecGraph, gpu_ok: bool) -> HashMap<StageId, Device> {
    let mut devs: HashMap<StageId, Device> = HashMap::new();
    for id in 0..g.node_count() {
        let h = g.stage(id).hints();
        match h.device {
            Device::Cpu => { devs.insert(id, Device::Cpu); }
            Device::Gpu => { devs.insert(id, if gpu_ok { Device::Gpu } else { Device::Cpu }); }
            Device::Either => {}
        }
    }
    let max_iter = g.node_count() * 3;
    let mut iter = 0;
    loop {
        iter += 1;
        let mut assigned = false;
        for id in 0..g.node_count() {
            if devs.contains_key(&id) { continue; }
            let hints = g.stage(id).hints();
            let mut adj_devs: Vec<Device> = Vec::new();
            for e in g.edges_out(id).chain(g.edges_in(id)) {
                let other = if e.from == id { e.to } else { e.from };
                if let Some(&d) = devs.get(&other) { adj_devs.push(d); }
            }
            if adj_devs.is_empty() { continue; }
            let first = adj_devs[0];
            let all_same = adj_devs.iter().all(|&d| d == first);
            if let Some(pref) = hints.preference {
                let effective = if pref == Device::Gpu && !gpu_ok { Device::Cpu } else { pref };
                devs.insert(id, effective);
                assigned = true;
                continue;
            }
            if all_same { devs.insert(id, first); assigned = true; continue; }
            devs.insert(id, if gpu_ok { Device::Gpu } else { Device::Cpu });
            assigned = true;
        }
        if !assigned { break; }
        if iter > max_iter {
            tracing::warn!("assign_devices: fixed-point did not converge after {iter} passes");
            for id in 0..g.node_count() { devs.entry(id).or_insert(if gpu_ok { Device::Gpu } else { Device::Cpu }); }
            break;
        }
    }
    for id in 0..g.node_count() { devs.entry(id).or_insert(if gpu_ok { Device::Gpu } else { Device::Cpu }); }
    devs
}

fn insert_transfers(g: &mut ExecGraph, devs: &mut HashMap<StageId, Device>) {
    let edges: Vec<_> = g.edges.iter().map(|e| (e.from, e.to, e.ports)).collect();
    for (src, dst, ports) in edges {
        let sd = devs[&src];
        let dd = devs[&dst];
        let data_kind = g.stage(src).output_ports().kind_at(ports.from_port as usize);
        let is_transferrable = data_kind.is_some_and(|k| k == crate::stage::DataKind::Tile || k == crate::stage::DataKind::Neighborhood);
        if sd != Device::Gpu && dd == Device::Gpu && is_transferrable {
            let mid = g.add_stage(Upload::stage());
            if let Some(ei) = g.find_edge(src, dst) { g.remove_edge(ei); }
            g.add_edge(src, mid, EdgePorts { from_port: ports.from_port, to_port: 0 });
            g.add_edge(mid, dst, EdgePorts { from_port: 0, to_port: ports.to_port });
            devs.insert(mid, Device::Cpu);
        } else if sd == Device::Gpu && dd != Device::Gpu && is_transferrable {
            let mid = g.add_stage(Download::stage());
            if let Some(ei) = g.find_edge(src, dst) { g.remove_edge(ei); }
            g.add_edge(src, mid, EdgePorts { from_port: ports.from_port, to_port: 0 });
            g.add_edge(mid, dst, EdgePorts { from_port: 0, to_port: ports.to_port });
            devs.insert(mid, Device::Cpu);
        }
    }
}

fn detect_chains(g: &ExecGraph, devs: &HashMap<StageId, Device>, order: &[StageId]) -> (Vec<Vec<StageId>>, HashMap<StageId, usize>) {
    let mut node_chain: HashMap<StageId, usize> = HashMap::new();
    let mut chains: Vec<Vec<StageId>> = Vec::new();
    for &id in order {
        let dev = devs[&id];
        let merged = if g.edges_in(id).count() == 1 {
            let pred = g.edges_in(id).next().unwrap().from;
            let pred_out = g.edges_out(pred).count();
            let same_device = (devs[&pred] != Device::Gpu) == (dev != Device::Gpu);
            if pred_out == 1 && same_device { Some(node_chain[&pred]) } else { None }
        } else { None };
        if let Some(ci) = merged { chains[ci].push(id); node_chain.insert(id, ci); }
        else { let ci = chains.len(); chains.push(vec![id]); node_chain.insert(id, ci); }
    }
    (chains, node_chain)
}

fn build_channels(g: &ExecGraph, order: &[StageId], node_chain: &HashMap<StageId, usize>, num_chains: usize) -> (Vec<Vec<ItemReceiver>>, Vec<Vec<(ItemSender, u16, u16)>>) {
    let mut ins: Vec<Vec<ItemReceiver>> = (0..num_chains).map(|_| vec![]).collect();
    let mut outs: Vec<Vec<(ItemSender, u16, u16)>> = (0..num_chains).map(|_| vec![]).collect();
    let mut seen = HashMap::new();
    for &src_node in order {
        let sc = node_chain[&src_node];
        for e in g.edges_out(src_node) {
            let dc = node_chain[&e.to];
            let ports = e.ports;
            if sc == dc || seen.contains_key(&(sc, dc, ports.from_port, ports.to_port)) { continue; }
            seen.insert((sc, dc, ports.from_port, ports.to_port), ());
            let (tx, rx) = sync_channel::<Option<RoutedItem>>(CHANNEL_BOUND);
            outs[sc].push((tx, ports.from_port, ports.to_port));
            ins[dc].push(rx);
        }
    }
    (ins, outs)
}

fn compute_work_total(g: &ExecGraph, order: &[StageId]) -> usize {
    let mut output_work: HashMap<StageId, usize> = HashMap::new();
    let mut total = 0;
    for &id in order {
        let input_work = g.edges_in(id).map(|e| output_work.get(&e.from).copied().unwrap_or(0)).max().unwrap_or(0);
        let received = if g.edges_in(id).count() == 0 { 1 } else { input_work };
        total += received;
        let ow = if g.edges_in(id).count() == 0 { g.stage(id).source_items() } else { (input_work as f64 * g.stage(id).work_multiplier()).ceil() as usize };
        output_work.insert(id, ow);
    }
        tracing::debug!("[pixors] compute_work_total: total_received={total}");
    total
}
