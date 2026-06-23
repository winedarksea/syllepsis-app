//! End-to-end check that the built-in WASM plugins load and run through the host. Requires the
//! `extism` feature and the staged `.wasm` artifacts (`plugins/build.sh`); skips cleanly if the
//! dist directory hasn't been built so a plain `cargo test` doesn't fail on a missing build step.

#![cfg(feature = "extism")]

use std::path::PathBuf;

use syllepsis_core::app::plugin as app_plugin;
use syllepsis_core::app::text_import::TextImportOptions;
use syllepsis_core::plugin::{PluginHost, PluginRegistry};

fn dist_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/dist")
        .canonicalize()
        .ok()?;
    dir.is_dir().then_some(dir)
}

#[test]
fn syntax_highlight_plugin_renders_html() {
    let Some(dist) = dist_dir() else {
        eprintln!("skipping: plugins/dist not built (run plugins/build.sh)");
        return;
    };
    let registry = PluginRegistry::discover(Some(&dist), None);
    assert!(
        registry.get("syntax-highlight").is_some(),
        "syntax-highlight plugin should be discovered in {dist:?}"
    );
    let host = PluginHost::load(&registry);
    let html = app_plugin::run_render_plugin(&host, &registry, "rust", "fn main() {}").unwrap();
    assert!(html.contains("tok-keyword"), "expected highlighted output, got: {html}");
    assert!(html.contains("fn"));
    // The renderer escapes input — a script tag must not survive as a live tag.
    let danger = app_plugin::run_render_plugin(&host, &registry, "rust", "<script>x</script>").unwrap();
    assert!(!danger.contains("<script>"), "renderer must escape angle brackets");
}

#[test]
fn pdf_import_plugin_executes_through_the_host() {
    let Some(dist) = dist_dir() else {
        eprintln!("skipping: plugins/dist not built (run plugins/build.sh)");
        return;
    };
    let registry = PluginRegistry::discover(Some(&dist), None);
    assert!(
        registry.get("pdf-import").is_some(),
        "pdf-import plugin should be discovered in {dist:?}"
    );
    let host = PluginHost::load(&registry);
    // Feeding non-PDF bytes proves the import call path runs the WASM (the plugin parses and
    // returns an error), which surfaces as a plugin error rather than panicking or hanging.
    let result = app_plugin::import_via_plugin(
        &host,
        &registry,
        "pdf-import",
        b"not a pdf",
        &TextImportOptions::default(),
    );
    assert!(result.is_err(), "invalid PDF bytes should produce a plugin error");
}
