#!/usr/bin/env bash
# Build rink-wasm and generate its JS bindings into the web app.
set -euo pipefail

cd "$(dirname "$0")/.."
target_dir="${CARGO_TARGET_DIR:-target}"

cargo build -p rink-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/src/lib/wasm "$target_dir/wasm32-unknown-unknown/release/rink_wasm.wasm"
