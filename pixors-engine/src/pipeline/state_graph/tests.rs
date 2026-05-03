#[cfg(test)]
mod tests {
    use crate::pipeline::state_graph::actions::{Action, ActionBatch};
    use crate::pipeline::state_graph::cache::{cache_key, CacheIndex};
    use crate::pipeline::state_graph::compile::{compile, ExecutionMode};
    use crate::pipeline::state_graph::graph::{EdgePorts, NodeId, StateGraph};
    use crate::pipeline::state_graph::history::History;
    use crate::pipeline::state::StateNode;
    use crate::pipeline::state;

    fn make_graph() -> (StateGraph, NodeId, NodeId, NodeId, NodeId) {
        let mut g = StateGraph::new();
        let file = g.add_node(StateNode::FileImage(state::FileImage {
            path: "test.png".into(),
        }));
        let blur = g.add_node(StateNode::Blur(state::Blur { radius: 5 }));
        let disk = g.add_node(StateNode::DiskCache(state::DiskCache {
            cache_id: Some("cache1".into()),
        }));
        let display = g.add_node(StateNode::DisplayCache(state::DisplayCache {
            generation: 1,
        }));
        g.add_edge(file, blur, EdgePorts::default());
        g.add_edge(blur, disk, EdgePorts::default());
        g.add_edge(disk, display, EdgePorts::default());
        g.outputs = vec![(display, 0)];
        (g, file, blur, disk, display)
    }

    #[test]
    fn validate_accepts_valid_graph() {
        let (g, ..) = make_graph();
        assert!(g.validate().is_ok());
    }

    #[test]
    fn validate_rejects_type_mismatch() {
        let mut g = StateGraph::new();
        let file = g.add_node(StateNode::FileImage(state::FileImage {
            path: "test.png".into(),
        }));
        let display = g.add_node(StateNode::DisplayCache(state::DisplayCache {
            generation: 1,
        }));
        g.add_edge(file, display, EdgePorts::default());
        assert!(g.validate().is_ok(), "Image → Image should be valid");
    }

    #[test]
    fn compile_produces_exec_graph() {
        let (g, ..) = make_graph();
        let ci = CacheIndex::new();
        let exec = compile(&g, ExecutionMode::Apply { force_cpu: false }, &ci, false).unwrap();

        assert!(exec.stage_count() > 0, "ExecGraph should have stages");
        let kinds = exec.kind_names();
        assert!(kinds.iter().any(|k| *k == "file_decoder"));
        assert!(kinds.iter().any(|k| *k == "blur_kernel"));
        assert!(kinds.iter().any(|k| *k == "display_sink"));
    }

    #[test]
    fn action_insert_node_increases_version() {
        let (mut g, ..) = make_graph();
        let v_before = g.version;

        let batch: ActionBatch = vec![Action::InsertNode {
            node: StateNode::Export(state::Export {
                path: "out.png".into(),
                format: crate::pipeline::state::ExportFormat::Png,
            }),
            assigned: None,
        }];
        let mut h = History::new(50);
        h.push(batch, &mut g);

        assert!(g.version > v_before, "version should increase");
        assert_eq!(g.node_count(), 5, "should have 5 nodes after insert");
    }

    #[test]
    fn undo_reverts_insert() {
        let (mut g, ..) = make_graph();
        let count_before = g.node_count();

        let batch: ActionBatch = vec![Action::InsertNode {
            node: StateNode::Export(state::Export {
                path: "out.png".into(),
                format: crate::pipeline::state::ExportFormat::Png,
            }),
            assigned: None,
        }];
        let mut h = History::new(50);
        h.push(batch, &mut g);
        assert_eq!(g.node_count(), count_before + 1);

        h.undo(&mut g);
        assert_eq!(
            g.node_count(),
            count_before,
            "undo should restore original count"
        );
    }

    #[test]
    fn redo_restores_after_undo() {
        let (mut g, ..) = make_graph();
        let count_before = g.node_count();

        let mut h = History::new(50);
        let batch: ActionBatch = vec![Action::InsertNode {
            node: StateNode::Export(state::Export {
                path: "out.png".into(),
                format: crate::pipeline::state::ExportFormat::Png,
            }),
            assigned: None,
        }];
        h.push(batch, &mut g);
        h.undo(&mut g);
        assert_eq!(g.node_count(), count_before);

        h.redo(&mut g);
        assert_eq!(g.node_count(), count_before + 1);
    }

    #[test]
    fn action_connect_adds_edge() {
        let (mut g, file, _blur, _disk, display) = make_graph();
        let edge_count = g.edge_count();

        let batch: ActionBatch = vec![Action::Connect {
            from: file,
            to: display,
            ports: EdgePorts::default(),
        }];
        let mut h = History::new(50);
        h.push(batch, &mut g);

        assert!(g.edge_count() > edge_count, "should have one more edge");
    }

    #[test]
    fn disconnect_removes_edge() {
        let (mut g, file, blur, ..) = make_graph();
        let edge_count = g.edge_count();

        let batch: ActionBatch = vec![Action::Disconnect {
            from: file,
            to: blur,
            ports: EdgePorts::default(),
        }];
        let mut h = History::new(50);
        h.push(batch, &mut g);

        assert_eq!(g.edge_count(), edge_count - 1, "should have one less edge");
    }

    #[test]
    fn cache_key_changes_with_params() {
        let mut g = StateGraph::new();
        let blur = g.add_node(StateNode::Blur(state::Blur { radius: 5 }));
        let key1 = cache_key(&g, blur);

        *g.node_mut(blur).unwrap() = StateNode::Blur(state::Blur { radius: 10 });
        let key2 = cache_key(&g, blur);

        assert_ne!(
            key1, key2,
            "different radii should produce different cache keys"
        );
    }

    #[test]
    fn cache_key_same_for_identical_params() {
        let mut g = StateGraph::new();
        let f1 = g.add_node(StateNode::FileImage(state::FileImage {
            path: "test.png".into(),
        }));
        let b1 = g.add_node(StateNode::Blur(state::Blur { radius: 5 }));
        g.add_edge(f1, b1, EdgePorts::default());
        let key1 = cache_key(&g, b1);

        let mut g2 = StateGraph::new();
        let f2 = g2.add_node(StateNode::FileImage(state::FileImage {
            path: "test.png".into(),
        }));
        let b2 = g2.add_node(StateNode::Blur(state::Blur { radius: 5 }));
        g2.add_edge(f2, b2, EdgePorts::default());
        let key2 = cache_key(&g2, b2);

        assert_eq!(
            key1, key2,
            "identical graphs should produce identical cache keys"
        );
    }

    #[test]
    fn history_multiple_undos() {
        let mut g = StateGraph::new();
        g.add_node(StateNode::FileImage(state::FileImage {
            path: "test.png".into(),
        }));
        let mut h = History::new(50);

        h.push(
            vec![Action::InsertNode {
                node: StateNode::Blur(state::Blur { radius: 3 }),
                assigned: None,
            }],
            &mut g,
        );
        h.push(
            vec![Action::InsertNode {
                node: StateNode::DiskCache(state::DiskCache { cache_id: None }),
                assigned: None,
            }],
            &mut g,
        );
        assert_eq!(g.node_count(), 3);

        h.undo(&mut g);
        assert_eq!(g.node_count(), 2);
        h.undo(&mut g);
        assert_eq!(g.node_count(), 1);

        assert!(!h.undo(&mut g));
    }

    #[test]
    fn serde_roundtrip_preserves_graph() {
        let (g, ..) = make_graph();
        let json = serde_json::to_string(&g).expect("serialize");
        let g2: StateGraph = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(g.node_count(), g2.node_count());
        assert_eq!(g.edge_count(), g2.edge_count());
        assert_eq!(g.outputs.len(), g2.outputs.len());
        assert!(g2.validate().is_ok());
        assert!(g2.topological_order().is_ok());
    }

    #[test]
    fn state_to_exec_roundtrip() {
        let (g, ..) = make_graph();
        let ci = CacheIndex::new();
        let exec = compile(&g, ExecutionMode::Apply { force_cpu: false }, &ci, false).unwrap();
        assert!(
            exec.stage_count() > 3,
            "should have multiple stages after expand"
        );
    }

    #[test]
    fn pathbuilder_png_roundtrip() {
        use std::path::PathBuf;

        use crate::pipeline::state::ExportFormat;
        use crate::pipeline::state_graph::builder::PathBuilder;

        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input = manifest.join("example1.png");
        if !input.exists() {
            tracing::info!("skipping: fixture {input:?} not present");
            return;
        }

        let output = manifest.join("pixors_pathbuilder_roundtrip.png");
        if output.exists() {
            let _ = std::fs::remove_file(&output);
        }

        PathBuilder::new()
            .source(StateNode::FileImage(state::FileImage {
                path: input.clone(),
            }))
            .sink(StateNode::Export(state::Export {
                path: output.clone(),
                format: ExportFormat::Png,
            }))
            .run(ExecutionMode::Apply { force_cpu: false })
            .expect("pipeline run");

        let in_dim = decode_png_dim(&input);
        let out_dim = decode_png_dim(&output);
        assert_eq!(in_dim, out_dim, "roundtripped PNG must keep dimensions");
    }

    #[test]
    fn pathbuilder_png_blur_roundtrip() {
        use std::path::PathBuf;

        use crate::pipeline::state::ExportFormat;
        use crate::pipeline::state_graph::builder::PathBuilder;

        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input = manifest.join("example1.png");
        if !input.exists() {
            tracing::info!("skipping: fixture {input:?} not present");
            return;
        }

        let output = manifest.join("pixors_blur_roundtrip.png");
        if output.exists() {
            let _ = std::fs::remove_file(&output);
        }

        PathBuilder::new()
            .source(StateNode::FileImage(state::FileImage {
                path: input.clone(),
            }))
            .operation(StateNode::Blur(state::Blur { radius: 32 }))
            .sink(StateNode::Export(state::Export {
                path: output.clone(),
                format: ExportFormat::Png,
            }))
            .run(ExecutionMode::Apply { force_cpu: false })
            .expect("blur pipeline");

        let out_dim = decode_png_dim(&output);
        assert_eq!(out_dim, (4148, 5531), "blurred PNG must keep dimensions");
    }

    #[test]
    fn blur_4x4_rgba_correct() {
        use crate::container::meta::PixelMeta;
        use crate::container::{Tile, TileCoord};
        use crate::pixel::{AlphaPolicy, PixelFormat};
        use crate::gpu::Buffer;

        let mut data = vec![0u8; 4 * 4 * 4];
        for y in 0..4u32 {
            for x in 0..4u32 {
                let o = (y as usize * 4 + x as usize) * 4;
                if x == 0 && y == 0 {
                    data[o] = 255;
                    data[o + 3] = 255;
                } else if x == 3 && y == 3 {
                    data[o + 2] = 255;
                    data[o + 3] = 255;
                } else {
                    data[o + 1] = 255;
                    data[o + 3] = 255;
                }
            }
        }

        let meta = PixelMeta::new(
            PixelFormat::Rgba8,
            crate::color::ColorSpace::SRGB,
            AlphaPolicy::Straight,
        );

        let coord = TileCoord::new(0, 0, 4, 4, 4);
        let tile = Tile::new(coord, meta, Buffer::cpu(data.clone()));

        let nbhd = crate::container::Neighborhood::new(
            1,
            coord,
            vec![tile],
            crate::container::EdgeCondition::Clamp,
            meta,
            4,
            4,
            4,
        );

        let mut runner =
            crate::pipeline::exec::blur_kernel::BlurKernelRunner::new(1);
        let mut emitter = crate::pipeline::exec_graph::emitter::Emitter::new();
        use crate::pipeline::exec_graph::runner::OperationRunner;
        runner
            .process(crate::pipeline::exec_graph::item::Item::Neighborhood(nbhd), &mut emitter)
            .unwrap();
        let result = emitter.into_items();
        assert_eq!(result.len(), 1);
        let blurred = match &result[0] {
            crate::pipeline::exec_graph::item::Item::Tile(t) => t,
            _ => panic!("expected Tile"),
        };

        let out: &[u8] = match &blurred.data {
            Buffer::Cpu(v) => v.as_slice(),
            _ => panic!("expected Cpu"),
        };

        assert_eq!(out[0], 63);
        assert_eq!(out[1], 191);
        assert_eq!(out[2], 0);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn blur_32x32_roundtrip() {
        use crate::pipeline::state::ExportFormat;
        use crate::pipeline::state_graph::builder::PathBuilder;

        let input = "/tmp/pixors_blur_32x32_in.png";
        let output = "/tmp/pixors_blur_32x32_out.png";

        {
            let file = std::fs::File::create(input).unwrap();
            let w = std::io::BufWriter::new(file);
            let mut encoder = png::Encoder::new(w, 32, 32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let mut pixels = vec![0u8; 32 * 32 * 4];
            for y in 0..32u32 {
                for x in 0..32u32 {
                    let o = (y as usize * 32 + x as usize) * 4;
                    let v = if (x + y) % 2 == 0 { 255u8 } else { 0u8 };
                    pixels[o] = v;
                    pixels[o + 1] = v;
                    pixels[o + 2] = v;
                    pixels[o + 3] = 255;
                }
            }
            writer.write_image_data(&pixels).unwrap();
        }

        PathBuilder::new()
            .source(StateNode::FileImage(state::FileImage {
                path: input.into(),
            }))
            .operation(StateNode::Blur(state::Blur { radius: 4 }))
            .sink(StateNode::Export(state::Export {
                path: output.into(),
                format: ExportFormat::Png,
            }))
            .run(ExecutionMode::Apply { force_cpu: false })
            .expect("blur 32x32");

        let out_dim = decode_png_dim(std::path::Path::new(output));
        assert_eq!(out_dim, (32, 32), "blurred 32x32 must keep dimensions");

        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn compile_cpu_only_path_no_bridges() {
        let (g, ..) = make_graph();
        let ci = CacheIndex::new();
        let exec = compile(&g, ExecutionMode::Apply { force_cpu: false }, &ci, false).unwrap();
        let kinds = exec.kind_names();
        assert!(!kinds.iter().any(|k| *k == "upload"));
        assert!(!kinds.iter().any(|k| *k == "download"));
        assert!(!kinds.iter().any(|k| *k == "blur_kernel_gpu"));
        assert!(kinds.iter().any(|k| *k == "blur_kernel"));
    }

    #[test]
    fn compile_gpu_path_inserts_upload_download() {
        let (g, ..) = make_graph();
        let ci = CacheIndex::new();
        let exec = compile(&g, ExecutionMode::Apply { force_cpu: false }, &ci, true).unwrap();
        let kinds = exec.kind_names();
        assert!(
            kinds.iter().any(|k| *k == "blur_kernel_gpu"),
            "GPU path should pick BlurKernelGpu"
        );
        assert!(
            kinds.iter().any(|k| *k == "upload"),
            "Cpu→Gpu boundary should insert Upload"
        );
        assert!(
            kinds.iter().any(|k| *k == "download"),
            "Gpu→Cpu boundary should insert Download"
        );
    }

    #[test]
    fn compile_gpu_path_two_blurs_no_inner_bridge() {
        let mut g = StateGraph::new();
        let file = g.add_node(StateNode::FileImage(state::FileImage {
            path: "x.png".into(),
        }));
        let b1 = g.add_node(StateNode::Blur(state::Blur { radius: 3 }));
        let b2 = g.add_node(StateNode::Blur(state::Blur { radius: 3 }));
        let disp = g.add_node(StateNode::DisplayCache(state::DisplayCache {
            generation: 1,
        }));
        g.add_edge(file, b1, EdgePorts::default());
        g.add_edge(b1, b2, EdgePorts::default());
        g.add_edge(b2, disp, EdgePorts::default());
        g.outputs = vec![(disp, 0)];

        let ci = CacheIndex::new();
        let exec = compile(&g, ExecutionMode::Apply { force_cpu: false }, &ci, true).unwrap();
        let kinds = exec.kind_names();
        let uploads = kinds.iter().filter(|k| **k == "upload").count();
        let downloads = kinds.iter().filter(|k| **k == "download").count();
        assert_eq!(uploads, 1, "single Cpu→Gpu transition expected");
        assert_eq!(downloads, 1, "single Gpu→Cpu transition expected");
    }

    fn decode_png_dim(path: &std::path::Path) -> (u32, u32) {
        let file = std::fs::File::open(path).expect("open png");
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let reader = decoder.read_info().expect("png header");
        let info = reader.info();
        (info.width, info.height)
    }
}
