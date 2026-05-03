use std::collections::{HashMap, VecDeque};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::visit::EdgeRef;

use crate::egraph::emitter::Emitter;
use crate::egraph::graph::{ExecGraph, StageId};
use crate::egraph::item::Item;
use crate::egraph::runner::{OperationRunner, SinkRunner, SourceRunner};
use crate::error::Error;

enum CompiledNode {
    Source(Box<dyn SourceRunner>),
    Operation(Box<dyn OperationRunner>),
    Sink(Box<dyn SinkRunner>),
}

/// Walks the `ExecGraph` in topological order, instantiating one runner per
/// stage and routing emitted `Item`s along outgoing edges.
pub struct Executor<'a> {
    graph: &'a ExecGraph,
    runners: HashMap<StageId, CompiledNode>,
}

impl<'a> Executor<'a> {
    pub fn new(graph: &'a ExecGraph) -> Result<Self, Error> {
        let mut runners = HashMap::with_capacity(graph.stage_count());
        for id in graph.graph.node_indices() {
            let stage = &graph.graph[id];
            let node = if let Ok(r) = stage.source_runner() {
                CompiledNode::Source(r)
            } else if let Ok(r) = stage.op_runner() {
                CompiledNode::Operation(r)
            } else if let Ok(r) = stage.sink_runner() {
                CompiledNode::Sink(r)
            } else {
                return Err(Error::internal(format!(
                    "no runner for stage: {}",
                    stage.kind()
                )));
            };
            runners.insert(id, node);
        }
        Ok(Self { graph, runners })
    }

    pub fn run(&mut self) -> Result<(), Error> {
        let order = toposort(&self.graph.graph, None).map_err(|_| Error::internal("cycle"))?;

        // Per-stage queue of items waiting to be processed.
        let mut pending: HashMap<StageId, VecDeque<Item>> = self
            .graph
            .graph
            .node_indices()
            .map(|id| (id, VecDeque::new()))
            .collect();

        for id in order {
            // Sources have no incoming edges: kick them off once.
            let is_source = self
                .graph
                .graph
                .edges_directed(id, Direction::Incoming)
                .next()
                .is_none();
            if is_source {
                let mut emitter = Emitter::new();
                if let Some(CompiledNode::Source(s)) = self.runners.get_mut(&id) {
                    s.run(&mut emitter)?;
                    s.finish(&mut emitter)?;
                }
                self.route(id, emitter.into_items(), &mut pending);
            }

            // Drain the stage's input queue.
            while let Some(item) = pending.get_mut(&id).and_then(|q| q.pop_front()) {
                let mut emitter = Emitter::new();
                match self.runners.get_mut(&id) {
                    Some(CompiledNode::Operation(o)) => o.process(item, &mut emitter)?,
                    Some(CompiledNode::Sink(s)) => s.consume(item)?,
                    _ => return Err(Error::internal("unexpected input")),
                }
                self.route(id, emitter.into_items(), &mut pending);
            }

            // Final flush.
            let mut emitter = Emitter::new();
            match self.runners.get_mut(&id) {
                Some(CompiledNode::Operation(o)) => o.finish(&mut emitter)?,
                Some(CompiledNode::Sink(s)) => s.finish()?,
                _ => {}
            }
            self.route(id, emitter.into_items(), &mut pending);
        }
        Ok(())
    }

    /// Push every emitted item onto the first downstream stage's queue.
    /// Matches the previous `break`-on-first-edge behaviour: an item flows
    /// to one successor, not all of them.
    fn route(
        &self,
        from: StageId,
        items: Vec<Item>,
        pending: &mut HashMap<StageId, VecDeque<Item>>,
    ) {
        let Some(target) = self
            .graph
            .graph
            .edges_directed(from, Direction::Outgoing)
            .next()
            .map(|er| er.target())
        else {
            return;
        };
        if let Some(queue) = pending.get_mut(&target) {
            for item in items {
                queue.push_back(item);
            }
        }
    }
}
