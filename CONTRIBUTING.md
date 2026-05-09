# Contributing to Pixors

## Before you start

Read [ARCHITECTURE.md](docs/ARCHITECTURE.md) — especially the pipeline invariants.
Violating them (processors calling wgpu directly, transfers in the wrong layer, etc.)
is a bug, not a style issue.

## Code style

- `cargo fmt --all` before every commit — non-negotiable
- `cargo clippy --workspace` before pushing — deny-lints break CI, fix them
- **Follow existing patterns**: look at neighboring files for naming, structure, idioms
- **No premature abstractions**: three similar lines beats a wrong abstraction
- **No comments explaining what the code does** — only add one when the *why* is non-obvious (hidden constraint, bug workaround, subtle invariant)
- **No error handling for impossible cases** — trust pipeline and framework guarantees; only validate at system boundaries

## Architecture rules

| Rule | Where it lives |
|---|---|
| No GUI deps (`iced`, `wgpu`, `rfd`) | `pixors-state` and below |
| No business logic, state mutations | `pixors-desktop` |
| Processors never call `wgpu::` directly | `pixors-engine` invariant |
| CPU↔GPU transfers only via `insert_transfers` | runtime auto-injects them |
| `Scheduler` is the only GPU API surface | no raw `wgpu::CommandEncoder` in processors |

See `CLAUDE.md` for the full list of pipeline invariants.

## Conventional commits

```
feat:     new user-visible feature
fix:      bug fix
refactor: code change with no behavior change
docs:     documentation only
chore:    tooling, deps, CI, formatting
```

Scope hint: `feat(phase10):`, `fix(blur):`, etc. when it helps.

## Branch strategy

- `main` — latest development state
- `feature/*` — feature branches; merge into `main` via PR or direct merge
- `release/X.Y.Z` — triggers CI build + GitHub release for all platforms

## Adding a new Stage

1. Implement `Stage` (+ `Producer`/`Processor`/`Consumer` as needed) in the right crate:
   - Color-related → `pixors-color`
   - Image I/O → `pixors-image`
   - Operations → `pixors-ops`
   - Viewport/display-only → `pixors-desktop`
2. If it needs GPU shaders, add `.slang` files to `pixors-shader/shaders/` and recompile with `slangc`.
3. Export from the crate's `lib.rs`.
4. Wire into `PathBuilder` or `ExecGraph` at the call site.

## Adding a new PixelFormat / ColorSpace

See the step-by-step guides in [CLAUDE.md](CLAUDE.md) — both involve touching several crates in a specific order.

## CI

CI runs on every push to `main` and on PRs:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

No frontend build — `pixors-desktop` is a native Iced app with no Node dependency.

Pre-compiled SPIR-V is checked into `pixors-shader/kernels/`. CI uses them directly
(no `slangc` required). Recompile locally with `slangc` when modifying `.slang` files
and commit the updated `.spv` binaries.
