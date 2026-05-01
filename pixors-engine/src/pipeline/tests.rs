#[cfg(test)]
mod tests {
    use crate::pipeline::converter::to_neighborhood::TileToNeighborhood;
    use crate::pipeline::graph::Graph;
    use crate::pipeline::node::{Node, NodeKind};
    use crate::pipeline::operation::blur::BlurOp;
    use crate::pipeline::path_builder::PathBuilder;
    use crate::pipeline::sink::file::ImageFileSink;
    use crate::pipeline::source::file::FileImageSource;

    #[test]
    fn blur_pipeline_types_compose() {
        let source = FileImageSource::new("test.png");
        let to_neighborhood = TileToNeighborhood::new(3);
        let blur = BlurOp::new(3);
        let sink = ImageFileSink::new("out.png");

        let graph = PathBuilder::from_source(source)
            .convert(to_neighborhood)
            .operation(blur)
            .sink(sink);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(
            graph.kinds(),
            vec![
                NodeKind::Source,
                NodeKind::Converter,
                NodeKind::Operation,
                NodeKind::Sink,
            ]
        );
    }

    #[test]
    fn scanline_pipeline() {
        let source = FileImageSource::new("test.png");
        let to_neighborhood = TileToNeighborhood::new(2);
        let blur = BlurOp::new(2);
        let sink = ImageFileSink::new("out.png");

        let graph = PathBuilder::from_source(source)
            .convert(to_neighborhood)
            .operation(blur)
            .sink(sink);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(
            graph.names(),
            vec![
                "file_image",
                "tile_to_neighborhood",
                "blur",
                "image_file_sink"
            ]
        );
    }

    #[test]
    fn build_without_sink() {
        let source = FileImageSource::new("test.png");
        let to_neighborhood = TileToNeighborhood::new(1);

        let graph = PathBuilder::from_source(source)
            .convert(to_neighborhood)
            .build();

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.outputs.len(), 1);
    }

    #[test]
    fn split_creates_independent_branches() {
        let source = FileImageSource::new("test.png");
        let to_tile = TileToNeighborhood::new(2);

        let [branch_a, branch_b] = PathBuilder::from_source(source).convert(to_tile).split();

        let graph_a = branch_a
            .operation(BlurOp::new(2))
            .sink(ImageFileSink::new("out_a.png"));

        let graph_b = branch_b
            .operation(BlurOp::new(5))
            .sink(ImageFileSink::new("out_b.png"));

        assert_eq!(graph_a.node_count(), 4);
        assert_eq!(graph_b.node_count(), 4);

        assert_eq!(
            graph_a.names(),
            vec![
                "file_image",
                "tile_to_neighborhood",
                "blur",
                "image_file_sink",
            ]
        );
        assert_eq!(
            graph_b.names(),
            vec![
                "file_image",
                "tile_to_neighborhood",
                "blur",
                "image_file_sink",
            ]
        );
    }

    #[test]
    fn valid_graph_passes_validation() {
        let source = FileImageSource::new("test.png");
        let to_neighborhood = TileToNeighborhood::new(3);
        let blur = BlurOp::new(3);
        let sink = ImageFileSink::new("out.png");

        let graph = PathBuilder::from_source(source)
            .convert(to_neighborhood)
            .operation(blur)
            .sink(sink);

        assert!(graph.validate().is_ok());
    }

    #[test]
    fn type_mismatch_is_detected() {
        use crate::pipeline::node::Node;

        let mut graph = Graph::new();
        graph.nodes.push(Node::from_converter(TileToNeighborhood::new(3)));
        graph.nodes.push(Node::from_sink(ImageFileSink::new("out.png")));
        graph.edges.push((0, 1));

        let errors = graph.validate().unwrap_err();
        assert_eq!(errors.len(), 1);
        let e = &errors[0];
        assert_eq!(e.from, 0);
        assert_eq!(e.to, 1);
        // TileToNeighborhood outputs Neighborhood, Sink expects Tile
        assert!(e.from_output.contains("Neighborhood"));
        assert!(e.to_input.contains("Tile"));
    }
}
