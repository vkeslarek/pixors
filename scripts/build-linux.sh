#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Building frontend ==="
cd pixors-ui && npm run build && cd ..

echo "=== Building engine (Linux) ==="
cargo build --release -p pixors-engine

echo "=== Building desktop (Linux) ==="
cargo build --release -p pixors-desktop

echo "=== Done ==="
ls -lh target/release/pixors-engine target/release/pixors-desktop
