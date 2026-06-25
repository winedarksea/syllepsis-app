#!/bin/sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
bundle_cache="$repository_root/crates/syllepsis-tauri/bundled-models"

cargo run \
  --manifest-path "$repository_root/Cargo.toml" \
  -p syllepsis-core \
  --features onnx \
  --example download_builtin_models \
  -- "$bundle_cache" embeddinggemma-300m
