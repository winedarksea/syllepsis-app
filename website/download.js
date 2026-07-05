// Progressive enhancement for the download section. Without JS the page still works:
// every asset link falls back to the GitHub releases page. With JS we:
//   1. wire each [data-asset] link to its concrete installer URL from downloads.json,
//   2. show file sizes and the release version/date,
//   3. point the hero CTA at the detected OS's best installer.

(async function () {
  function detectOS() {
    const ua = navigator.userAgent;
    const platform = navigator.platform || "";
    if (/Android/i.test(ua)) return "android";
    if (/Win/i.test(platform) || /Windows/i.test(ua)) return "windows";
    if (/Linux/i.test(platform) && !/Android/i.test(ua)) return "linux";
    if (/Mac/i.test(platform) || /Mac OS X/i.test(ua)) {
      // Apple Silicon reports as "MacIntel" too; default to arm64 (current hardware).
      return "macos";
    }
    return null;
  }

  // Best default installer per detected OS.
  const CTA_ASSET = {
    macos: ["macos_arm64", "macos_x64"],
    windows: ["windows_exe", "windows_msi"],
    linux: ["linux_appimage", "linux_deb", "linux_rpm"],
    android: ["android_apk"],
  };
  const CTA_LABEL = {
    macos: "Download for macOS",
    windows: "Download for Windows",
    linux: "Download for Linux",
    android: "Download for Android",
  };

  function formatSize(bytes) {
    if (!bytes) return "";
    const mb = bytes / (1024 * 1024);
    return mb >= 1 ? `${mb.toFixed(0)} MB` : `${(bytes / 1024).toFixed(0)} KB`;
  }

  let data;
  try {
    const res = await fetch("/downloads.json", { cache: "no-cache" });
    data = await res.json();
  } catch (_) {
    return; // no-JS fallback links remain valid
  }

  const assets = data.assets || {};

  // 1 + 2: wire concrete URLs and sizes into the per-platform table.
  document.querySelectorAll("a[data-asset]").forEach((a) => {
    const asset = assets[a.dataset.asset];
    if (asset && asset.url) {
      a.href = asset.url;
      if (asset.size) {
        const span = document.createElement("span");
        span.className = "size";
        span.textContent = formatSize(asset.size);
        a.after(span);
      }
    }
  });

  // Version / date line.
  const meta = document.getElementById("downloads-meta");
  if (meta && data.version) {
    const date = data.published_at ? new Date(data.published_at).toLocaleDateString() : "";
    meta.textContent = `Version ${data.version}${date ? " · " + date : ""} · free & open source`;
  }

  const allLink = document.getElementById("all-releases-link");
  if (allLink && data.all_releases_url) allLink.href = data.all_releases_url;

  // 3: retarget the hero CTA at the detected OS.
  const cta = document.getElementById("download-cta");
  const os = detectOS();
  if (cta && os && CTA_ASSET[os]) {
    const key = CTA_ASSET[os].find((k) => assets[k] && assets[k].url);
    if (key) {
      cta.href = assets[key].url;
      cta.textContent = CTA_LABEL[os];
      const sub = document.getElementById("download-cta-sub");
      if (sub && assets[key].size) {
        sub.textContent = `${assets[key].name} · ${formatSize(assets[key].size)} · other platforms below`;
      }
    }
  }
})();
