#!/usr/bin/env node
// Generate website/downloads.json by querying the latest published GitHub Release and mapping
// its assets to per-platform download URLs. Prints JSON to stdout.
//
//   node scripts/website-downloads.mjs > website/downloads.json
//
// If no published release exists yet (releases/latest 404s), the committed fallback
// website/downloads.json is echoed unchanged so the site always has something to link to.
//
// Auth is optional (public repo); set GH_TOKEN to raise the rate limit.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const REPO = process.env.GITHUB_REPOSITORY || "winedarksea/syllepsis-app";
const __dirname = dirname(fileURLToPath(import.meta.url));
const FALLBACK = join(__dirname, "..", "website", "downloads.json");

// Ordered: first matching pattern wins per platform key. Patterns are matched against the
// asset file name (case-insensitive). Keep in sync with the Downloads table in index.html.
const PATTERNS = [
  ["macos_arm64", /aarch64.*\.dmg$/i],
  ["macos_x64", /(x64|x86_64).*\.dmg$/i],
  ["windows_exe", /(x64_)?setup\.exe$/i],
  ["windows_msi", /\.msi$/i],
  ["linux_appimage", /\.appimage$/i],
  ["linux_deb", /\.deb$/i],
  ["linux_rpm", /\.rpm$/i],
  ["android_apk", /\.apk$/i],
];

function emitFallback(reason) {
  process.stderr.write(`website-downloads: ${reason}; using committed fallback\n`);
  process.stdout.write(readFileSync(FALLBACK, "utf8"));
}

async function main() {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "syllepsis-website" };
  if (process.env.GH_TOKEN) headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;

  let release;
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers });
    if (res.status === 404) return emitFallback("no published release");
    if (!res.ok) return emitFallback(`GitHub API ${res.status}`);
    release = await res.json();
  } catch (err) {
    return emitFallback(`fetch failed: ${err.message}`);
  }

  const assets = {};
  for (const [key, pattern] of PATTERNS) {
    if (assets[key]) continue;
    const asset = (release.assets || []).find((a) => pattern.test(a.name));
    if (asset) {
      assets[key] = {
        url: asset.browser_download_url,
        name: asset.name,
        size: asset.size,
      };
    }
  }

  const out = {
    version: (release.tag_name || "").replace(/^v/, ""),
    tag: release.tag_name || "",
    published_at: release.published_at || "",
    html_url: release.html_url || `https://github.com/${REPO}/releases`,
    all_releases_url: `https://github.com/${REPO}/releases`,
    assets,
  };
  process.stdout.write(JSON.stringify(out, null, 2) + "\n");
}

main();
