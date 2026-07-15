#!/usr/bin/env node
// Gemma's Q2 external data is small enough after xz compression to fit below GitHub Releases'
// 2 GiB asset cap. The desktop app extracts this archive into its normal model cache at startup.
import { mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const repositoryRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const bundledModelsDirectory = join(
  repositoryRoot,
  "crates",
  "syllepsis-tauri",
  "bundled-models",
);
const archiveDirectory = join(
  repositoryRoot,
  "crates",
  "syllepsis-tauri",
  "bundled-model-archives",
);
const archivePath = join(archiveDirectory, "models.tar.xz");

mkdirSync(archiveDirectory, { recursive: true });
const result = spawnSync(
  "tar",
  ["-cJf", archivePath, "-C", bundledModelsDirectory, "."],
  { stdio: "inherit", shell: false },
);

if (result.error) {
  console.error(result.error);
  process.exit(1);
}
process.exit(result.status ?? 1);
