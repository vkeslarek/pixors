# Overview

## Purpose

Pixors is an open-source image editor (MIT) written in Rust. Three delivery modes:

- **Standalone application** — full-featured image editor UI
- **Library** — embeddable image processing engine
- **MCP server** — Model Context Protocol integration for AI workflows _(deferred)_

API-first: programmatic interface comes first. Bindings, CLI, GUI, MCP plug on top later.

## Core Principles

1. **Efficiency** — process images of any size using GPU and CPU appropriately
2. **Adaptive resource allocation** — route operations based on throughput, with aggressive caching
3. **Asynchronous API** — never block the caller; prioritize user-perceived responsiveness

## Scope

### In scope (engine)

- 2D raster images, very large sizes (gigapixel-class), tiled processing
- HDR/wide-gamut working space
- Deferred operation graphs with lazy execution
- Priority-driven scheduling with viewport awareness
- CPU and GPU compute backends with automatic offloading
- Layers, masks, selections, non-destructive history
- Common file formats (PNG, JPEG, TIFF, EXR, …)
- ICC-color-managed import/export

### Out of scope (for now)

- Vector graphics (may revisit)
- Video
- 3D / raytracing
- Tile-level fault tolerance (a failing tile fails the whole job)

### Deferred

- HDR tone mapping for SDR preview
- MCP server
- Language bindings (Python, JS)
- CLI, GUI
- Plugin system

## Key Architectural Choices

- **Three-tier storage**: `DISK <-> CPU <-> GPU`, strict — no DISK↔GPU bypass
- **Tile-based**: fixed-size tiles, rectangular neighborhoods only
- **Deferred graph**: operations chain into a graph, executes on `ctx.run()`
- **Priority scheduling**: viewport-interactive work preempts background
- **GPU via Vulkan compute**: storage buffers only (no texture units)

See [DECISIONS](DECISIONS.md) for rationale.
