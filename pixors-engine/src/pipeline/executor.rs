use std::any::Any;
use std::collections::VecDeque;

use crate::error::Error;
use crate::pipeline::graph::Graph;
use crate::pipeline::node::Node;

pub fn run(graph: &mut Graph) -> Result<(), Error> {
    if let Err(errors) = graph.validate() {
        return Err(Error::internal(format!(
            "validation failed: {} mismatch(es)",
            errors.len()
        )));
    }

    let order = topo_sort(graph)?;
    let n = graph.node_count();
    let mut pending: Vec<VecDeque<Box<dyn Any>>> = (0..n).map(|_| VecDeque::new()).collect();

    for idx in order {
        if graph.edges.iter().all(|(_, to)| *to != idx) {
            let mut buf = Vec::new();
            let mut emit = |item: Box<dyn Any>| buf.push(item);
            match &mut graph.nodes[idx] {
                Node::Source(s) => {
                    s.run_cpu_erased(&mut emit)?;
                    s.finish_cpu_erased(&mut emit)?;
                }
                _ => {}
            }
            route(idx, buf, graph, &mut pending);
        }

        while let Some(item) = pending[idx].pop_front() {
            let mut buf = Vec::new();
            let mut emit = |item: Box<dyn Any>| buf.push(item);
            match &mut graph.nodes[idx] {
                Node::Converter(c) => c.process_cpu_erased(item, &mut emit)?,
                Node::Operation(o) => o.process_cpu_erased(item, &mut emit)?,
                Node::Sink(s) => s.consume_cpu_erased(item)?,
                Node::Source(_) => return Err(Error::internal("source cannot receive input")),
            }
            route(idx, buf, graph, &mut pending);
        }

        let mut buf = Vec::new();
        let mut emit = |item: Box<dyn Any>| buf.push(item);
        match &mut graph.nodes[idx] {
            Node::Converter(c) => c.finish_cpu_erased(&mut emit)?,
            Node::Operation(o) => o.finish_cpu_erased(&mut emit)?,
            Node::Sink(s) => s.finish_cpu_erased()?,
            _ => {}
        }
        route(idx, buf, graph, &mut pending);
    }

    Ok(())
}

fn route(
    from: usize,
    items: Vec<Box<dyn Any>>,
    graph: &Graph,
    pending: &mut [VecDeque<Box<dyn Any>>],
) {
    for item in items {
        for &(f, to) in &graph.edges {
            if f == from {
                pending[to].push_back(item);
                break;
            }
        }
    }
}

fn topo_sort(graph: &Graph) -> Result<Vec<usize>, Error> {
    let n = graph.node_count();
    let mut in_degree = vec![0; n];
    for &(_, to) in &graph.edges {
        in_degree[to] += 1;
    }

    let mut queue: VecDeque<usize> = (0..n).filter(|i| in_degree[*i] == 0).collect();
    let mut order = Vec::new();

    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &(from, to) in &graph.edges {
            if from == idx {
                in_degree[to] -= 1;
                if in_degree[to] == 0 {
                    queue.push_back(to);
                }
            }
        }
    }

    if order.len() != n {
        return Err(Error::internal("graph contains a cycle"));
    }

    Ok(order)
}
