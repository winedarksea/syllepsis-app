# Built-in Syllepsis plugins

These are the two reference plugins for the WASM plugin system. Both are written in Rust and
compiled to `wasm32-unknown-unknown`, then run by the host (`syllepsis-core::plugin::host`,
behind the `extism` feature) via [Extism](https://extism.org).

| Plugin | Kind | Export | What it does |
|---|---|---|---|
| `syntax-highlight` | `code_block_renderer` | `render({language, code}) -> {html}` | Highlights fenced code blocks to sanitized HTML, shown in place of the raw block. |
| `pdf-import` | `import_source` | `import(bytes) -> {text}` | Extracts plain text from a PDF and feeds it into the Note Importer. |

## Building

```sh
./plugins/build.sh
```

This stages each plugin under `plugins/dist/<plugin-id>/` (`plugin.json` + the `.wasm`). The
`.wasm` artifacts are **not** checked in — build them locally (or in CI) before running the app.

## Running the app against locally-built plugins

The app discovers built-in plugins from its bundled resource directory in a release build. For
`cargo tauri dev`, override the location with an env var:

```sh
export SYLLEPSIS_PLUGIN_DIR="$(pwd)/plugins/dist"
```

User-installed plugins are additionally discovered from `<app_data_dir>/plugins/` and override a
built-in with the same id.

## The plugin contract

A plugin is a directory containing a `plugin.json` manifest (see
`syllepsis-core::plugin::manifest`) and its entry `.wasm`. Plugins change notes only through the
note-write host functions (`create_note`, `replace_body`), which the host routes through the normal
note-edit path so changes flow into the CRDT sidecar at sync time.
