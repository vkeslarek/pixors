.PHONY: build-front build-engine build-desktop build check test lint clean release-linux release-windows release-macos

# ── Frontend ──────────────────────────────────────────
build-front:
	cd pixors-ui && npm run build

# ── Rust (dev) ────────────────────────────────────────
build-engine:
	cargo build --release -p pixors-engine

build-desktop:
	cargo build --release -p pixors-desktop

build: build-engine build-desktop

check:
	cargo check --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings

clean:
	cargo clean
	rm -rf pixors-ui/dist

# ── Release builds ────────────────────────────────────
release-linux: build-front
	cargo build --release -p pixors-engine
	cargo build --release -p pixors-desktop

release-windows: build-front
	cargo build --release -p pixors-engine --target x86_64-pc-windows-gnu
	cargo build --release -p pixors-desktop --target x86_64-pc-windows-gnu

release-macos: build-front
	cargo build --release -p pixors-engine --target aarch64-apple-darwin
	cargo build --release -p pixors-desktop --target aarch64-apple-darwin
