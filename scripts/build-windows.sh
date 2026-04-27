#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="x86_64-pc-windows-gnu"

echo "=== Building frontend ==="
cd pixors-ui && npm run build && cd ..

echo "=== Building engine (Windows) ==="
cargo build --release -p pixors-engine --target "$TARGET"

echo "=== Building desktop (Windows) ==="
cargo build --release -p pixors-desktop --target "$TARGET"

echo "=== Done ==="
ls -lh "target/$TARGET/release/pixors-engine.exe" "target/$TARGET/release/pixors-desktop.exe" "target/$TARGET/release/WebView2Loader.dll" 2>/dev/null || true
