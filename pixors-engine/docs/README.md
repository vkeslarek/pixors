# Pixors — Design Docs

Open-source image editor in Rust. MIT licensed. Runs as standalone app, embeddable library, and (eventually) MCP server.

## Navigation

- [OVERVIEW](OVERVIEW.md) — vision, core principles, scope
- [DATA_MODEL](DATA_MODEL.md) — pixel format, color space, bit depth, alpha
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — DISK/CPU/GPU engines, transfers
- [TILE_SYSTEM](TILE_SYSTEM.md) — tiles, neighborhood, work units
- [MIP_PYRAMID](MIP_PYRAMID.md) — multi-resolution pyramid
- [OPERATION_GRAPH](OPERATION_GRAPH.md) — `ValueId`, graph, lazy eval, fusion
- [OPERATIONS](OPERATIONS.md) — categories, constraints, capability matrix
- [EXECUTION_MODEL](EXECUTION_MODEL.md) — jobs, work units, execution plan
- [SCHEDULER](SCHEDULER.md) — priority, QoS, adaptive offload
- [EDITOR_SEMANTICS](EDITOR_SEMANTICS.md) — layers, masks, selections, undo/history _(TBD)_
- [API](API.md) — programmatic interface, `Context`, callbacks _(TBD)_
- [ROADMAP](ROADMAP.md) — implementation phases
- [DECISIONS](DECISIONS.md) — cross-cutting decisions log with rationale
- [REVIEW](REVIEW.md) — design review round 1: simplifications, holes, risks
- [PHASE1](PHASE1.md) — detailed Phase 1 spec (image I/O, types, color model)

## Status

Design phase. Source under `src/` is scaffold only. No engine work started.

## Contributing

Design docs evolve via discussion. Decisions locked in [DECISIONS](DECISIONS.md) should not be reopened without new evidence.
