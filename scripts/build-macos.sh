#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="aarch64-apple-darwin"

echo "=== Building frontend ==="
cd pixors-ui && npm run build && cd ..

echo "=== Building engine (macOS ARM) ==="
cargo build --release -p pixors-engine --target "$TARGET"

echo "=== Building desktop (macOS ARM) ==="
cargo build --release -p pixors-desktop --target "$TARGET"

echo "=== Done ==="
ls -lh "target/$TARGET/release/pixors-engine" "target/$TARGET/release/pixors-desktop"
