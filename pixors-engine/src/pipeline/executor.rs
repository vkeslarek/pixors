use crate::error::Error;
use crate::pipeline::graph::Graph;
use crate::pipeline::node::Node;

pub fn run(graph: &mut Graph) -> Result<(), Error> {
    let node_count = graph.node_count();
    if node_count == 0 {
        return Ok(());
    }

    let mut node_outputs: Vec<Vec<Box<dyn std::any::Any + Send>>> = vec![Vec::new(); node_count];

    for &(from, to) in &graph.edges {
        if node_outputs[from].is_empty() {
            let outputs = execute_node(&mut graph.nodes[from])?;
            node_outputs[from] = outputs;
        }

        for item in node_outputs[from].drain(..) {
            feed_node(&mut graph.nodes[to], item)?;
        }
    }

    for idx in graph.outputs.iter() {
        if node_outputs[*idx].is_empty() {
            node_outputs[*idx] = execute_node(&mut graph.nodes[*idx])?;
        }
    }

    for node in graph.nodes.iter_mut() {
        finish_node(node)?;
    }

    Ok(())
}

fn execute_node(node: &mut Node) -> Result<Vec<Box<dyn std::any::Any + Send>>, Error> {
    match node {
        Node::Source(s) => {
            let mut out = Vec::new();
            let mut emit = |item: Box<dyn std::any::Any + Send>| out.push(item);
            s.run_cpu_erased(&mut emit)?;
            s.finish_cpu_erased(&mut emit)?;
            Ok(out)
        }
        Node::Converter(c) | Node::Operation(c) => {
            // Converters and operations need input from upstream.
            // If called as "source" (no input), return empty.
            Ok(Vec::new())
        }
        Node::Sink(_) => Ok(Vec::new()),
    }
}

fn feed_node(node: &mut Node, item: Box<dyn std::any::Any + Send>) -> Result<(), Error> {
    match node {
        Node::Converter(c) => c.consume_input_erased(item),
        Node::Operation(o) => o.consume_input_erased(item),
        Node::Sink(s) => s.consume_cpu_erased(item),
        Node::Source(_) => Err(Error::internal("source cannot receive input")),
    }
}

fn finish_node(node: &mut Node) -> Result<(), Error> {
    match node {
        Node::Converter(c) => {
            let mut out = Vec::new();
            let mut emit = |item: Box<dyn std::any::Any + Send>| out.push(item);
            c.finish_cpu_erased(&mut emit)?;
            Ok(())
        }
        Node::Operation(o) => {
            let mut out = Vec::new();
            let mut emit = |item: Box<dyn std::any::Any + Send>| out.push(item);
            o.finish_cpu_erased(&mut emit)?;
            Ok(())
        }
        Node::Sink(s) => s.finish_cpu_erased(),
        Node::Source(_) => {
            let mut out = Vec::new();
            let mut emit = |item: Box<dyn std::any::Any + Send>| out.push(item);
            // Source already done in execute_node, noop here
            Ok(())
        }
    }
}
