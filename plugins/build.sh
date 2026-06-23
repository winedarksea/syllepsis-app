#!/usr/bin/env bash
# Build the built-in Syllepsis WASM plugins and stage them where the app can discover them.
#
# Output layout (the dir the registry scans): plugins/dist/<plugin-id>/{plugin.json, <entry>.wasm}
# For `cargo tauri dev`, point the app at it:  export SYLLEPSIS_PLUGIN_DIR="$(pwd)/plugins/dist"
# For a release bundle, this dist dir is what tauri.conf.json's bundle.resources ships.
set -euo pipefail

cd "$(dirname "$0")"
# wasm32-wasip1 (not bare wasm32-unknown-unknown): the host enables WASI, so transitive deps that
# need an entropy source (getrandom, via lopdf's hashers) resolve to the WASI backend at runtime.
TARGET="wasm32-wasip1"
DIST="dist"

rustup target add "$TARGET" >/dev/null 2>&1 || true
mkdir -p "$DIST"

# crate-dir  crate-name(snake)  plugin-id  entry-wasm
build_plugin() {
  local crate_dir="$1" crate_name="$2" plugin_id="$3" entry="$4"
  echo "==> building $plugin_id"
  (cd "$crate_dir" && cargo build --release --target "$TARGET")
  local out="$DIST/$plugin_id"
  mkdir -p "$out"
  cp "$crate_dir/target/$TARGET/release/$crate_name.wasm" "$out/$entry"
  cp "$crate_dir/plugin.json" "$out/plugin.json"
  echo "    staged $out/$entry"
}

build_plugin "syntax-highlight" "syntax_highlight" "syntax-highlight" "syntax_highlight.wasm"
build_plugin "pdf-import"       "pdf_import"       "pdf-import"       "pdf_import.wasm"

echo "Done. Plugins staged in plugins/$DIST"
