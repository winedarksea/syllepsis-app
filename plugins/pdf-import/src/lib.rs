//! A PDF text-extractor compiled to WASM and run by the Syllepsis plugin host as an
//! `import_source`. It demonstrates the import hook: given the raw bytes of a `.pdf`, it returns
//! the document's plain text, which the host feeds into the existing text-import preview→chunk→
//! commit pipeline so the user can split it into notes.

use extism_pdk::*;
use lopdf::Document;
use serde::Serialize;

#[derive(Serialize)]
struct ImportOutput {
    text: String,
}

#[plugin_fn]
pub fn import(data: Vec<u8>) -> FnResult<Json<ImportOutput>> {
    let doc = Document::load_mem(&data)?;
    let mut text = String::new();
    // Pages are keyed by page number; iterate in order so the extracted text reads top-to-bottom.
    let mut page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
    page_numbers.sort_unstable();
    for number in page_numbers {
        if let Ok(page_text) = doc.extract_text(&[number]) {
            let trimmed = page_text.trim();
            if !trimmed.is_empty() {
                text.push_str(trimmed);
                text.push_str("\n\n");
            }
        }
    }
    Ok(Json(ImportOutput {
        text: text.trim_end().to_string(),
    }))
}
