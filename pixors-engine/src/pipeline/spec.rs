use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::pipeline::converter::to_neighborhood::TileToNeighborhood;
use crate::pipeline::converter::to_tile::ScanLineToTile;
use crate::pipeline::graph::Graph;
use crate::pipeline::node::Node;
use crate::pipeline::operation::blur::BlurOp;
use crate::pipeline::sink::file::ImageFileSink;
use crate::pipeline::source::file::FileImageSource;

#[derive(Serialize, Deserialize)]
struct GraphWire {
    version: u32,
    nodes: Vec<NodeWire>,
    edges: Vec<(String, String)>,
    #[serde(default)]
    outputs: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct NodeWire {
    id: String,
    kind: String,
    #[serde(default)]
    params: serde_json::Value,
}

impl Graph {
    pub fn to_json(&self) -> serde_json::Value {
        let nodes: Vec<NodeWire> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| NodeWire {
                id: format!("n{}", i),
                kind: n.name().to_string(),
                params: n.params(),
            })
            .collect();

        let edges: Vec<(String, String)> = self
            .edges
            .iter()
            .map(|(from, to)| (format!("n{}", from), format!("n{}", to)))
            .collect();

        let outputs: Vec<String> = self
            .outputs
            .iter()
            .map(|i| format!("n{}", i))
            .collect();

        let wire = GraphWire {
            version: 1,
            nodes,
            edges,
            outputs,
        };

        serde_json::to_value(wire).unwrap()
    }

    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(&self.to_json()).unwrap()
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, Error> {
        let wire: GraphWire = serde_json::from_value(json.clone()).map_err(|e| {
            Error::invalid_param(format!("invalid pipeline JSON: {}", e))
        })?;

        if wire.version != 1 {
            return Err(Error::invalid_param(format!(
                "unsupported pipeline version: {}",
                wire.version
            )));
        }

        let mut id_to_idx: HashMap<String, usize> = HashMap::new();
        let mut nodes: Vec<Node> = Vec::new();

        for nw in &wire.nodes {
            let node = build_node(&nw.kind, &nw.params)?;
            let idx = nodes.len();
            nodes.push(node);
            id_to_idx.insert(nw.id.clone(), idx);
        }

        let edges: Vec<(usize, usize)> = wire
            .edges
            .iter()
            .map(|(from, to)| {
                let fi = *id_to_idx.get(from).ok_or_else(|| {
                    Error::invalid_param(format!("edge references unknown node: {}", from))
                })?;
                let ti = *id_to_idx.get(to).ok_or_else(|| {
                    Error::invalid_param(format!("edge references unknown node: {}", to))
                })?;
                Ok((fi, ti))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let outputs: Vec<usize> = wire
            .outputs
            .iter()
            .map(|id| {
                id_to_idx
                    .get(id)
                    .copied()
                    .ok_or_else(|| Error::invalid_param(format!("output references unknown node: {}", id)))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(Graph {
            nodes,
            edges,
            outputs,
        })
    }
}

fn build_node(kind: &str, params: &serde_json::Value) -> Result<Node, Error> {
    match kind {
        "file_image" => {
            let src: FileImageSource = serde_json::from_value(params.clone())
                .map_err(|e| Error::invalid_param(format!("file_image: {}", e)))?;
            Ok(Node::from_source(src))
        }
        "blur" => {
            let op: BlurOp = serde_json::from_value(params.clone())
                .map_err(|e| Error::invalid_param(format!("blur: {}", e)))?;
            Ok(Node::from_operation(op))
        }
        "scanline_to_tile" => {
            let conv: ScanLineToTile = serde_json::from_value(params.clone())
                .map_err(|e| Error::invalid_param(format!("scanline_to_tile: {}", e)))?;
            Ok(Node::from_converter(conv))
        }
        "tile_to_neighborhood" => {
            let conv: TileToNeighborhood = serde_json::from_value(params.clone())
                .map_err(|e| Error::invalid_param(format!("tile_to_neighborhood: {}", e)))?;
            Ok(Node::from_converter(conv))
        }
        "image_file_sink" => {
            let sink: ImageFileSink = serde_json::from_value(params.clone())
                .map_err(|e| Error::invalid_param(format!("image_file_sink: {}", e)))?;
            Ok(Node::from_sink(sink))
        }
        other => Err(Error::invalid_param(format!("unknown node kind: {}", other))),
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::converter::to_neighborhood::TileToNeighborhood;
    use crate::pipeline::graph::Graph;
    use crate::pipeline::operation::blur::BlurOp;
    use crate::pipeline::path_builder::PathBuilder;
    use crate::pipeline::sink::file::ImageFileSink;
    use crate::pipeline::source::file::FileImageSource;

    #[test]
    fn graph_roundtrip_json() {
        let source = FileImageSource::new("test.png");
        let converter = TileToNeighborhood::new(3);
        let blur = BlurOp::new(3);
        let sink = ImageFileSink::new("out.png");

        let graph = PathBuilder::from_source(source)
            .convert(converter)
            .operation(blur)
            .sink(sink);

        let json = graph.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let graph2 = Graph::from_json(&parsed).unwrap();

        assert_eq!(graph.node_count(), graph2.node_count());
        assert_eq!(graph.edge_count(), graph2.edge_count());
        assert_eq!(graph.names(), graph2.names());
        assert_eq!(graph.kinds(), graph2.kinds());
    }

    #[test]
    fn split_graph_roundtrip_json() {
        let source = FileImageSource::new("test.png");
        let converter = TileToNeighborhood::new(2);
        let sink_a = ImageFileSink::new("out_a.png");
        let sink_b = ImageFileSink::new("out_b.png");

        let [branch_a, branch_b] = PathBuilder::from_source(source)
            .convert(converter)
            .split();

        let mut graph_a = branch_a.operation(BlurOp::new(2)).sink(sink_a);
        let graph_b = branch_b.operation(BlurOp::new(5)).sink(sink_b);

        let offset = graph_a.nodes.len();
        for node in graph_b.nodes {
            graph_a.nodes.push(node);
        }
        for (from, to) in graph_b.edges {
            graph_a.edges.push((from + offset, to + offset));
        }
        graph_a.outputs.clear();

        let json = graph_a.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let round_tripped = Graph::from_json(&parsed).unwrap();

        assert_eq!(round_tripped.node_count(), graph_a.node_count());
        assert_eq!(round_tripped.edge_count(), graph_a.edge_count());
        assert_eq!(round_tripped.names(), graph_a.names());
        assert_eq!(round_tripped.kinds(), graph_a.kinds());
    }

    #[test]
    fn invalid_edge_still_deserializes() {
        let json = serde_json::json!({
            "version": 1,
            "nodes": [
                {"id": "n0", "kind": "tile_to_neighborhood", "params": {"radius": 3}},
                {"id": "n1", "kind": "image_file_sink", "params": {"path": "out.png"}}
            ],
            "edges": [["n0", "n1"]],
            "outputs": ["n1"]
        });

        let result = Graph::from_json(&json);
        assert!(result.is_ok(), "deserialization should succeed even with type-mismatched edges");
        let graph = result.unwrap();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }
}
