use std::collections::{HashMap, VecDeque};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::visit::EdgeRef;
use crate::data::buffer::Buffer;
use crate::data::tile::{Tile, TileCoord};
use crate::graph::emitter::Emitter;
use crate::graph::graph::{ExecGraph, StageId};
use crate::graph::item::Item;
use crate::graph::routed::Routed;
use crate::model::pixel::meta::PixelMeta;
use crate::model::pixel::{AlphaPolicy, PixelFormat};
use crate::stage::{Processor, ProcessorContext, Stage};
use crate::error::Error;
use crate::debug_stopwatch;
use crate::model::color::space::ColorSpace;

enum CompiledNode {
    Kernel(Box<dyn Processor>),
}

pub struct Executor<'a> {
    graph: &'a ExecGraph,
    nodes: HashMap<StageId, CompiledNode>,
}

impl<'a> Executor<'a> {
    pub fn new(graph: &'a ExecGraph) -> Result<Self, Error> {
        let mut nodes = HashMap::with_capacity(graph.stage_count());
        for id in graph.graph.node_indices() {
            let stage = &graph.graph[id];
            let node = if let Some(k) = stage.processor() {
                CompiledNode::Kernel(k)
            } else {
                return Err(Error::internal(format!(
                    "no processor for stage: {}",
                    stage.kind()
                )));
            };
            nodes.insert(id, node);
        }
        Ok(Self { graph, nodes })
    }

    pub fn run(&mut self) -> Result<(), Error> {
        let order = toposort(&self.graph.graph, None).map_err(|_| Error::internal("cycle"))?;

        let mut pending: HashMap<StageId, VecDeque<Routed<Item>>> = self
            .graph
            .graph
            .node_indices()
            .map(|id| (id, VecDeque::new()))
            .collect();

        for id in order {
            let stage = &self.graph.graph[id];
            let _sw = debug_stopwatch!(format!("stage:{}", stage.kind()));
            let is_source = self
                .graph
                .graph
                .edges_directed(id, Direction::Incoming)
                .next()
                .is_none();
            if is_source {
                let mut emitter = Emitter::new();
                if let Some(CompiledNode::Kernel(k)) = self.nodes.get_mut(&id) {
                    let dummy = Item::Tile(Tile::new(
                        TileCoord::new(0, 0, 0, 0, 0, 0),
                        PixelMeta::new(
                            PixelFormat::Rgba8,
                            ColorSpace::SRGB,
                            AlphaPolicy::Straight,
                        ),
                        Buffer::cpu(vec![]),
                    ));
                    k.process(ProcessorContext { port: 0, device: stage.device(), emit: &mut emitter }, dummy)?;
                    k.finish(ProcessorContext { port: 0, device: stage.device(), emit: &mut emitter })?;
                }
                self.route(id, emitter.into_items(), &mut pending);
            }

            while let Some(item) = pending.get_mut(&id).and_then(|q| q.pop_front()) {
                let mut emitter = Emitter::new();
                match self.nodes.get_mut(&id) {
                    Some(CompiledNode::Kernel(k)) => k.process(ProcessorContext { port: item.port, device: stage.device(), emit: &mut emitter }, item.payload)?,
                    _ => return Err(Error::internal("unexpected input")),
                }
                self.route(id, emitter.into_items(), &mut pending);
            }

            let mut emitter = Emitter::new();
            match self.nodes.get_mut(&id) {
                Some(CompiledNode::Kernel(k)) => k.finish(ProcessorContext { port: 0, device: stage.device(), emit: &mut emitter })?,
                _ => {}
            }
            self.route(id, emitter.into_items(), &mut pending);
        }
        Ok(())
    }

    fn route(
        &self,
        from: StageId,
        items: Vec<Routed<Item>>,
        pending: &mut HashMap<StageId, VecDeque<Routed<Item>>>,
    ) {
        for edge in self.graph.graph.edges_directed(from, Direction::Outgoing) {
            let target = edge.target();
            let from_port = edge.weight().from_port;
            let to_port = edge.weight().to_port;
            if let Some(queue) = pending.get_mut(&target) {
                for item in &items {
                    if item.port == from_port {
                        queue.push_back(Routed {
                            port: to_port,
                            payload: item.payload.clone(),
                        });
                    }
                }
            }
        }
    }
}
