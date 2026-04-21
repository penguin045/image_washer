#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCS_DIR="$ROOT_DIR/docs"
PKG_DIR="$DOCS_DIR/pkg"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen is required. Install it with:" >&2
  echo "cargo install wasm-bindgen-cli --version 0.2.118 --locked" >&2
  exit 1
fi

PYTHON_BIN="${PYTHON_BIN:-}"
if [ -z "$PYTHON_BIN" ] && [ -x /usr/bin/python3 ]; then
  PYTHON_BIN="/usr/bin/python3"
elif [ -z "$PYTHON_BIN" ]; then
  PYTHON_BIN="python3"
fi

cd "$ROOT_DIR"

cargo build --release --lib --target wasm32-unknown-unknown
rm -rf "$DOCS_DIR"
mkdir -p "$PKG_DIR"
cp web/index.html web/app.js web/styles.css web/manifest.webmanifest web/sw.js "$DOCS_DIR/"
touch "$DOCS_DIR/.nojekyll"
"$PYTHON_BIN" scripts/generate-icons.py "$DOCS_DIR/icons"

wasm-bindgen \
  --target web \
  --no-typescript \
  --out-dir "$PKG_DIR" \
  target/wasm32-unknown-unknown/release/image_washer.wasm
