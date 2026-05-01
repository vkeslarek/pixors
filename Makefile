.PHONY: build check test lint clean

# ── Rust (dev) ────────────────────────────────────────
build:
	cargo build --release -p pixors-desktop

check:
	cargo check --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings

clean:
	cargo clean
