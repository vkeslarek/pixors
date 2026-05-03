# GPU Eliding — Phase 4: Fusion Detection Pass

## Where to Put the Fusion Pass

After `state_graph/compile.rs` returns an `ExecGraph`, run a post-compile pass
that scans for consecutive `BlurKernelGpu` nodes and replaces them with a
single `FusedBlurKernelGpu`.

The pass is implemented as a free function in `exec_graph/fusion.rs` and
called from `state_graph/compile.rs`.

## File: `pixors-engine/src/pipeline/exec_graph/fusion.rs`

Replace the **entire file** with this implementation.  The current file
references `crate::gpu::kernel::{GpuKernel, KernelClass, KernelSig}` (a path
that doesn't exist) and uses `body_wgsl` (a field that doesn't exist on
`KernelSig`).  Discard it completely.

```rust
use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::visit::EdgeRef;

use crate::pipeline::exec::{ExecNode, Stage};
use crate::pipeline::exec_graph::graph::{ExecEdgePorts, ExecGraph, StageId};

/// Walk the ExecGraph and fuse runs of adjacent BlurKernelGpu nodes into
/// FusedBlurKernelGpu nodes.  Returns a new graph.
///
/// A "run" is a maximal sequence of BlurKernelGpu nodes where each is the
/// sole successor of the previous.  Runs of length 1 are left unchanged.
pub fn fuse_gpu_kernels(graph: &ExecGraph) -> ExecGraph {
    // Find maximal chains of consecutive BlurKernelGpu nodes.
    let chains = find_blur_chains(graph);
    if chains.iter().all(|c| c.len() < 2) {
        // Nothing to fuse — return a clone via rebuilding.
        return rebuild_unchanged(graph);
    }

    // Build a replacement table: StageId → which chain index it belongs to,
    // and whether it is the "head" (first) node of its chain.
    let mut in_chain: std::collections::HashMap<StageId, (usize, usize)> =
        std::collections::HashMap::new();
    for (ci, chain) in chains.iter().enumerate() {
        for (pos, &sid) in chain.iter().enumerate() {
            in_chain.insert(sid, (ci, pos));
        }
    }

    let mut new_graph = ExecGraph::new();
    // Map from old StageId → new StageId (for re-wiring edges).
    let mut id_map: std::collections::HashMap<StageId, StageId> =
        std::collections::HashMap::new();
    // For each chain: remember the new node representing the whole chain.
    let mut chain_node: std::collections::HashMap<usize, StageId> =
        std::collections::HashMap::new();

    let topo = toposort(&graph.graph, None).expect("no cycle in exec graph");

    for old_id in &topo {
        let node = &graph.graph[*old_id];
        if let Some(&(ci, pos)) = in_chain.get(old_id) {
            let chain = &chains[ci];
            if chain.len() < 2 {
                // Single-node "chain" — pass through unchanged.
                let new_id = new_graph.add_stage(node.clone());
                id_map.insert(*old_id, new_id);
            } else if pos == 0 {
                // Head of a multi-node chain — emit FusedBlurKernelGpu.
                let radii: Vec<u32> = chain
                    .iter()
                    .map(|&sid| {
                        if let ExecNode::BlurKernelGpu(ref b) = graph.graph[sid] {
                            b.radius
                        } else {
                            unreachable!()
                        }
                    })
                    .collect();
                let fused = crate::pipeline::exec::FusedBlurKernelGpu { radii };
                let new_id = new_graph.add_stage(ExecNode::FusedBlurKernelGpu(fused));
                for &sid in chain {
                    id_map.insert(sid, new_id);
                }
                chain_node.insert(ci, new_id);
            }
            // Non-head chain nodes: already mapped above (id_map points to chain head).
        } else {
            let new_id = new_graph.add_stage(node.clone());
            id_map.insert(*old_id, new_id);
        }
    }

    // Re-wire edges, deduplicating.
    let mut added_edges: std::collections::HashSet<(StageId, StageId)> =
        std::collections::HashSet::new();
    for old_id in &topo {
        for er in graph.graph.edges_directed(*old_id, Direction::Outgoing) {
            let src = id_map[old_id];
            let tgt = id_map[&er.target()];
            if src != tgt && added_edges.insert((src, tgt)) {
                new_graph.add_edge(src, tgt, ExecEdgePorts::default());
            }
        }
    }

    // Re-map output ports.
    new_graph.outputs = graph
        .outputs
        .iter()
        .filter_map(|(old_id, port)| {
            Some((*id_map.get(old_id)?, *port))
        })
        .collect();

    new_graph
}

/// Find maximal chains of consecutive BlurKernelGpu nodes.
/// A chain: each node has exactly one successor, which is also a BlurKernelGpu,
/// and that successor has exactly one predecessor.
fn find_blur_chains(graph: &ExecGraph) -> Vec<Vec<StageId>> {
    let topo = toposort(&graph.graph, None).expect("no cycle");
    let mut visited: std::collections::HashSet<StageId> = std::collections::HashSet::new();
    let mut chains: Vec<Vec<StageId>> = Vec::new();

    for &sid in &topo {
        if visited.contains(&sid) { continue; }
        if !is_blur_gpu(graph, sid) { continue; }

        // Only start a chain at a node that is NOT a continuation of a previous chain.
        // i.e., its predecessor (if any) is not a BlurKernelGpu.
        let pred_is_blur = graph
            .graph
            .edges_directed(sid, Direction::Incoming)
            .any(|er| is_blur_gpu(graph, er.source()));
        if pred_is_blur { continue; }

        // Extend the chain forward.
        let mut chain = vec![sid];
        visited.insert(sid);
        let mut cur = sid;
        loop {
            let succs: Vec<StageId> = graph
                .graph
                .edges_directed(cur, Direction::Outgoing)
                .map(|er| er.target())
                .collect();
            if succs.len() != 1 { break; }
            let next = succs[0];
            if !is_blur_gpu(graph, next) { break; }
            // next must have exactly one predecessor (cur).
            let preds: Vec<_> = graph
                .graph
                .edges_directed(next, Direction::Incoming)
                .collect();
            if preds.len() != 1 { break; }
            chain.push(next);
            visited.insert(next);
            cur = next;
        }
        chains.push(chain);
    }
    chains
}

fn is_blur_gpu(graph: &ExecGraph, sid: StageId) -> bool {
    matches!(graph.graph[sid], ExecNode::BlurKernelGpu(_))
}

/// Rebuild graph unchanged (used when no fusion is needed, avoids clone of
/// the petgraph which doesn't implement Clone).
fn rebuild_unchanged(graph: &ExecGraph) -> ExecGraph {
    let topo = toposort(&graph.graph, None).expect("no cycle");
    let mut new_graph = ExecGraph::new();
    let mut id_map: std::collections::HashMap<StageId, StageId> =
        std::collections::HashMap::new();
    for &old_id in &topo {
        let new_id = new_graph.add_stage(graph.graph[old_id].clone());
        id_map.insert(old_id, new_id);
    }
    for &old_id in &topo {
        for er in graph.graph.edges_directed(old_id, Direction::Outgoing) {
            let src = id_map[&old_id];
            let tgt = id_map[&er.target()];
            new_graph.add_edge(src, tgt, ExecEdgePorts::default());
        }
    }
    new_graph.outputs = graph
        .outputs
        .iter()
        .filter_map(|(old_id, port)| Some((*id_map.get(old_id)?, *port)))
        .collect();
    new_graph
}
```

## Register the fusion module in `exec_graph/mod.rs`

Current `exec_graph/mod.rs`:
```rust
pub mod emitter;
pub mod executor;
pub mod graph;
pub mod item;
pub mod runner;
```

Add:
```rust
pub mod fusion;
```

## Call the Fusion Pass from `state_graph/compile.rs`

At the bottom of the `compile` function, **before** returning `Ok(exec)`:

```rust
// Fuse adjacent GPU kernels.
let exec = crate::pipeline::exec_graph::fusion::fuse_gpu_kernels(&exec);

Ok(exec)
```

The full modified tail of `compile()`:

```rust
    exec.outputs = spec
        .outputs
        .iter()
        .filter_map(|(node, port)| {
            let &(_, last) = node_endpoints.get(node)?;
            Some((last, *port))
        })
        .collect();

    // Post-compile fusion pass.
    let exec = crate::pipeline::exec_graph::fusion::fuse_gpu_kernels(&exec);

    Ok(exec)
```

## Verify Fusion Works End-to-End

Add this test to `pixors-engine/src/pipeline/state_graph/tests.rs` (or a new
file `exec_graph/tests.rs`):

```rust
#[test]
fn double_blur_fuses() {
    use crate::pipeline::state::{Blur, DisplayCache, FileImage, StateNode};
    use crate::pipeline::state_graph::builder::PathBuilder;
    use crate::pipeline::state_graph::compile::{compile, ExecutionMode};
    use crate::pipeline::state_graph::cache::CacheIndex;
    use crate::pipeline::exec::ExecNode;

    let graph = PathBuilder::new()
        .source(StateNode::FileImage(FileImage {
            path: std::path::PathBuf::from("test.png"),
        }))
        .operation(StateNode::Blur(Blur { radius: 8 }))
        .operation(StateNode::Blur(Blur { radius: 8 }))
        .sink(StateNode::DisplayCache(DisplayCache { generation: 0 }))
        .build();

    let cache = CacheIndex::default();
    let exec = compile(&graph, ExecutionMode::Apply { force_cpu: false }, &cache, true)
        .expect("compile");

    // After fusion, no BlurKernelGpu should remain; one FusedBlurKernelGpu should exist.
    let kinds: Vec<&str> = exec.graph.node_indices()
        .map(|i| exec.graph[i].kind())
        .collect();
    assert!(!kinds.contains(&"blur_kernel_gpu"),
        "BlurKernelGpu should have been fused; got: {:?}", kinds);
    assert!(kinds.contains(&"fused_blur_kernel_gpu"),
        "FusedBlurKernelGpu should be present; got: {:?}", kinds);
}
```

## Verify

```bash
cargo check --workspace
cargo test -p pixors-engine -- double_blur_fuses
```

## Cleanup: Remove Dead Code in `fusion.rs`

The old `fusion.rs` stub referenced `GpuKernel`, `KernelSig`, `body_wgsl`,
and other non-existent symbols.  The replacement above uses only
`ExecNode::BlurKernelGpu` pattern-matching — no external trait imports.

The `fusable_body()` method in `pixors-shader/src/kernel.rs` and the `GpuKernel`
trait's `fusable_body` method can be **removed** (they are no longer needed
since fusion is driven by the exec graph structure, not by WGSL body strings).

Remove from `pixors-shader/src/kernel.rs`:
```rust
// DELETE this method from GpuKernel trait:
fn fusable_body(&self) -> Option<&'static str> {
    None
}
```
