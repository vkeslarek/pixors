# Operation Graph

The deferred computation graph. Users build the graph by calling methods on a `Context`; nothing executes until `run()`. The graph is a strict DAG of typed nodes with uniform dataflow: everything that comes out of one node can feed any other node, including as a parameter.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Graph shape | Strict DAG |
| Identifier | Single typed ID per value (no wrapper/inner distinction) |
| Value types | `Image`, `Meta<T>`, `MetaBuffer<T>` |
| Dataflow uniformity | Parameters and inputs are the same thing — both are upstream `ValueId`s |
| Constants | Literal Rust values are wrapped as a `Meta` node at construction time |
| Iteration | No cycles — iterative computations are built as explicit repeated nodes |
| Fusion policy | Same engine + same tile topology → single kernel; otherwise split |

## Value types

### `Image`

Tiled, pyramided RGBA `f16` interleaved (see [DATA_MODEL](DATA_MODEL.md), [TILE_SYSTEM](TILE_SYSTEM.md), [MIP_PYRAMID](MIP_PYRAMID.md)). The heavy type — carries a full storage engine footprint and eviction/pinning machinery.

### `Meta<T>`

Small fixed-size data. Constraints:

- `T: Sized` — compile-time-known size
- `T` is POD (bit-for-bit reinterpretable; `bytemuck::Pod` or equivalent)
- `T: Copy`
- No heap indirection

Uses: scalars, transform matrices, fixed-size color vectors, small per-op config structs, kernel coefficients for known-radius kernels, …

**GPU representation**: bound as a **uniform buffer**, straight byte-copy upload.

Examples:
- `Meta<f32>` — scalar
- `Meta<[f32; 9]>` — 3×3 transform matrix
- `Meta<BlurParams>` where `BlurParams { radius: f32, sigma: f32 }` is `Pod`

### `MetaBuffer<T>`

Dynamic-length array of POD elements. Constraints on `T` are the same as `Meta<T>`, but length is runtime.

Uses: lists of interest points, histogram bins, match tables, keypoint pairs, …

**GPU representation**: bound as a **storage buffer** (SSBO). Size known at bind time.

Examples:
- `MetaBuffer<Point2D>` — list of 2D points
- `MetaBuffer<[u32; 256]>` — per-bin RGB histograms
- `MetaBuffer<Match>` — correspondence pairs

### Common shape

Everything users pass around is a `ValueId<T>` where `T` is one of these three families. Ops consume and produce them.

## Nodes

A graph node records:

```
Node {
    id: NodeId,
    op: OpKind,                 // enum of all operation kinds
    inputs: Vec<ValueId<?>>,    // upstream values feeding this node
    outputs: Vec<ValueId<?>>,   // values produced by this node
    constraints: OpConstraints, // neighborhood, engine preference, etc
}
```

Inputs include anything the op needs: images, metadata, parameters. A node **does not** carry a separate "param struct" — everything is an upstream value. If the user passed a Rust literal, the `Context` implicitly created a small constant `Meta` node and fed its ID in.

Construction looks like:

```rust
let ctx = Context::new();
let image = ctx.load_image("in.png");
let blurred = ctx.blur(image, 5.0);            // 5.0 auto-wrapped into Meta<f32>
let radius = ctx.auto_blur_radius(image);      // ValueId<Meta<f32>>
let blurred2 = ctx.blur(image, radius);        // same signature, dynamic param
ctx.save(blurred, "out.png");
let job = ctx.run();
```

Both calls to `ctx.blur` use the same shape: the "param" is just another input.

## Uniform dataflow — consequences

### Constants

A literal Rust value passed where a `ValueId` is expected is wrapped internally as a constant `Meta` node. This node is a no-op at execution time (its output is pre-computed, just the literal bytes). Constant-folding passes can see through it.

### Dynamic parameters

An op can depend on the result of another op as a parameter. For example, computing an auto-levels-style contrast factor and using it:

```rust
let stats = ctx.image_statistics(image);             // Meta<Stats>
let factor = ctx.auto_contrast_factor(stats);        // Meta<f32>
let corrected = ctx.contrast(image, factor);
```

The scheduler waits for `factor` to materialize before dispatching the `contrast` work units.

### No distinction between "hyperparameter" and "input"

This is deliberate. It means any part of an op's configuration can itself be the output of upstream computation without the graph needing a second-class parameter mechanism.

## DAG

The graph is a strict directed acyclic graph.

### Why no cycles

An iterative computation does not need a cycle — each iteration is its own set of nodes, consuming the previous iteration's output. Four iterations of a bilateral filter = four `bilateral` nodes chained, not one node self-looping. This makes:

- Scheduling and dependency analysis trivial (topological order exists)
- Fusion analysis local and predictable
- Debugging tractable — every value has a single producer

### Convergence-based loops

For ops that need a stop condition (e.g. "iterate until delta < ε"), the construction happens outside the graph: the user runs a sub-graph, inspects the result, decides whether to build another iteration. This puts the control flow in the host language, where it belongs.

## Execution model at a glance

See [EXECUTION_MODEL](EXECUTION_MODEL.md) _(TBD)_ for the full detail. High level:

1. User finishes building → `ctx.run()`
2. Graph is optimized (fusion pass, constant folding, dead-code elimination)
3. Nodes are compiled into work units (tile × neighborhood expansion)
4. Work units are scheduled by priority on the available engines
5. Results materialize on the output handles; job completion events fire

## Fusion policy

Baseline policy — deliberately simple:

> Two consecutive ops fuse into a single kernel dispatch if they run on the **same engine** and operate on the **same tile topology** (same MIP level, compatible neighborhood). Otherwise the boundary is a separate dispatch.

### What fuses

- `brighten → contrast → invert` on GPU: all per-pixel, same engine → one compute shader
- `color_matrix → gamma_encode` on CPU: all per-pixel, same engine → one CPU kernel
- Per-pixel ops between two neighborhood ops: fuse the per-pixel chain; dispatch separately around the neighborhood ops

### What splits

- Op A on GPU, op B on CPU: forced download between them
- Op A with neighborhood `(1,1,1,1)`, op B with neighborhood `(3,3,3,3)`: different tile access patterns — separate
- Op A producing `Meta<T>`, op B consuming it: `Meta` materializes, then B runs
- Fork in the graph (output used by 2+ consumers): intermediate materializes once; each consumer starts a new fusion frontier

### Constant folding

A literal-constant `Meta` input becomes a compile-time specialization of the kernel: the value is baked into the shader / CPU code instead of being bound as a uniform.

### Higher ambition later

The baseline covers a large fraction of realistic pipelines. More aggressive fusion (across different neighborhoods, across engine boundaries with async transfer overlap) is a future optimization. Revisit when there is a concrete workload showing the baseline is a bottleneck.

## Graph invariants

- Every `ValueId` has exactly one producing node (or is a graph input)
- Every `ValueId` type is known at construction time (compile-time type, when Rust expresses it)
- Every node's input types match its declared input signature (checked at graph-building time)
- Every node's output IDs are newly-allocated (no reuse across producers)
- The set of graph inputs (load ops, literal constants) and outputs (save ops, explicit sinks) is finite and known before `run()`

## Ops defined, not yet specified

A separate [OPERATIONS](OPERATIONS.md) doc catalogs individual operations, their constraints, and their MIP-aware parameter scaling rules. Reference stubs:

- **Pixel**: brightness, contrast, invert, gamma, color matrix, gain
- **Local**: Gaussian blur, box blur, sharpen, unsharp mask, edge detect (DoG / Sobel)
- **Global**: histogram, image statistics, auto-levels factors
- **Geometric**: resize, rotate, crop, warp (perspective, affine)
- **Compositing**: Porter–Duff blend, mask apply, layer merge
- **File I/O**: load, save (per-format variants)

## Relations

- [API](API.md) — user-facing `Context` surface _(TBD)_
- [OPERATIONS](OPERATIONS.md) — per-op contracts, constraints, MIP scaling _(TBD)_
- [EXECUTION_MODEL](EXECUTION_MODEL.md) — how the graph becomes work units and dispatches _(TBD)_
- [SCHEDULER](SCHEDULER.md) — priority-aware dispatching, resource allocation _(TBD)_
