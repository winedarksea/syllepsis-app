#!/usr/bin/env node
// Cross-platform entry point for preparing the bundled ONNX models.
//
// Tauri runs `beforeBuildCommand` through the platform shell (cmd.exe on Windows),
// which cannot execute the sibling `.sh` script. This Node wrapper works identically
// on Windows, macOS, and Linux and simply shells out to the same `cargo run` the shell
// script does. Keep this in sync with prepare-bundled-models.sh.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const repositoryRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const bundleCache = join(
  repositoryRoot,
  "crates",
  "syllepsis-tauri",
  "bundled-models",
);

const result = spawnSync(
  "cargo",
  [
    "run",
    "--manifest-path",
    join(repositoryRoot, "Cargo.toml"),
    "-p",
    "syllepsis-core",
    "--features",
    "onnx",
    "--example",
    "download_builtin_models",
    "--",
    bundleCache,
    "embeddinggemma-300m",
  ],
  { stdio: "inherit", shell: false },
);

if (result.error) {
  console.error(result.error);
  process.exit(1);
}
process.exit(result.status ?? 1);
