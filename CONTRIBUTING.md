# Contributing to Pixors

## Code style

- **Cargo fmt**: always run `cargo fmt` before committing.
- **Clippy**: lint levels are defined in `Cargo.toml`. Run `cargo clippy --workspace` before pushing. Deny-lints break CI — fix them.
- **Well thought abstractions** make the code easy to read, too many abstractions make it unreadable.
- **Follow existing patterns**: look at neighboring files for naming, structure, and idioms.

## Conventional commits

```
feat: description
fix: description
docs: description
chore: description
refactor: description
```

## Branch strategy

- `main` — latest development state
- `feature/*` — feature branches, merge into `main` via PR
- `release/X.Y.Z` — triggers CI build + GitHub release for all platforms
