//! Chunking a text-import preview into LLM-sized pieces.
//!
//! Chunks pack consecutive items sharing a `category_context`, so outline sections stay intact
//! and the model sees coherent topical text. Pure function: the tauri layer exposes it as a
//! command and drives the actual LLM jobs.

use serde::{Deserialize, Serialize};

use super::TextImportPreviewItem;

pub const DEFAULT_CHUNK_MAX_CHARS: usize = 6_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextImportLlmChunk {
    pub index: usize,
    /// Section heading (category context) shared by the chunk's items, if any.
    pub heading: Option<String>,
    /// The chunk text handed to the model.
    pub text: String,
    /// Preview item indices (positions in the given slice) covered by this chunk.
    pub item_indices: Vec<usize>,
    pub warnings: Vec<String>,
}

pub fn chunk_items_for_llm(
    items: &[TextImportPreviewItem],
    max_chars: usize,
) -> Vec<TextImportLlmChunk> {
    let max_chars = if max_chars == 0 {
        DEFAULT_CHUNK_MAX_CHARS
    } else {
        max_chars
    };
    let mut chunks: Vec<TextImportLlmChunk> = Vec::new();
    let mut current_text = String::new();
    let mut current_indices: Vec<usize> = Vec::new();
    let mut current_heading: Option<String> = None;

    let flush = |chunks: &mut Vec<TextImportLlmChunk>,
                 text: &mut String,
                 indices: &mut Vec<usize>,
                 heading: &Option<String>| {
        if indices.is_empty() {
            return;
        }
        chunks.push(TextImportLlmChunk {
            index: chunks.len(),
            heading: heading.clone(),
            text: std::mem::take(text),
            item_indices: std::mem::take(indices),
            warnings: Vec::new(),
        });
    };

    for (item_index, item) in items.iter().enumerate() {
        let piece = item_text(item);
        let piece_len = piece.chars().count();

        if piece_len > max_chars {
            // Oversized single item: it becomes its own chunk(s), split on line boundaries.
            flush(
                &mut chunks,
                &mut current_text,
                &mut current_indices,
                &current_heading,
            );
            for (part_number, part) in split_on_lines(&piece, max_chars).into_iter().enumerate() {
                let mut warnings = Vec::new();
                if part_number == 0 {
                    warnings.push(format!(
                        "Item {} exceeds the chunk size and was split across chunks; the model sees it in pieces.",
                        item_index + 1
                    ));
                }
                chunks.push(TextImportLlmChunk {
                    index: chunks.len(),
                    heading: item.category_context.clone(),
                    text: part,
                    item_indices: vec![item_index],
                    warnings,
                });
            }
            current_heading = None;
            continue;
        }

        let same_section = current_indices.is_empty() || current_heading == item.category_context;
        let fits = current_text.chars().count() + piece_len + 2 <= max_chars;
        if !same_section || !fits {
            flush(
                &mut chunks,
                &mut current_text,
                &mut current_indices,
                &current_heading,
            );
        }
        if current_indices.is_empty() {
            current_heading = item.category_context.clone();
        }
        if !current_text.is_empty() {
            current_text.push_str("\n\n");
        }
        current_text.push_str(&piece);
        current_indices.push(item_index);
    }
    flush(
        &mut chunks,
        &mut current_text,
        &mut current_indices,
        &current_heading,
    );
    chunks
}

fn item_text(item: &TextImportPreviewItem) -> String {
    let title = item.title.trim();
    let body = item.body.trim();
    if title.is_empty() || body.starts_with(title) {
        body.to_string()
    } else {
        format!("{title}\n{body}")
    }
}

fn split_on_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line_len = line.chars().count();
        if !current.is_empty() && current.chars().count() + line_len + 1 > max_chars {
            parts.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        // A single line longer than max_chars still goes through whole: line boundaries are the
        // finest split we do (mid-line splits would corrupt the text the model must preserve).
        current.push_str(line);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.is_empty() {
        parts.push(text.to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::super::{TextImportBlockKind, TextImportPreviewItem};
    use super::*;

    fn item(index: usize, category: Option<&str>, body: &str) -> TextImportPreviewItem {
        TextImportPreviewItem {
            index,
            title: String::new(),
            body: body.to_string(),
            block_kind: TextImportBlockKind::Paragraph,
            category_context: category.map(ToString::to_string),
            intended_prior: None,
            warnings: Vec::new(),
            categories: Vec::new(),
            parent_index: None,
            depth: 0,
        }
    }

    #[test]
    fn packs_consecutive_items_of_the_same_section() {
        let items = vec![
            item(0, Some("garden"), "one"),
            item(1, Some("garden"), "two"),
            item(2, Some("turbine"), "three"),
        ];
        let chunks = chunk_items_for_llm(&items, 6_000);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("garden"));
        assert_eq!(chunks[0].item_indices, vec![0, 1]);
        assert_eq!(chunks[0].text, "one\n\ntwo");
        assert_eq!(chunks[1].heading.as_deref(), Some("turbine"));
        assert_eq!(chunks[1].item_indices, vec![2]);
    }

    #[test]
    fn splits_when_the_size_budget_is_exceeded() {
        let items = vec![
            item(0, None, &"a".repeat(40)),
            item(1, None, &"b".repeat(40)),
            item(2, None, &"c".repeat(40)),
        ];
        let chunks = chunk_items_for_llm(&items, 100);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].item_indices, vec![0, 1]);
        assert_eq!(chunks[1].item_indices, vec![2]);
        assert_eq!(chunks[1].index, 1);
    }

    #[test]
    fn oversized_single_item_becomes_its_own_warned_chunks() {
        let long_body = (0..10)
            .map(|i| format!("line {i} {}", "x".repeat(30)))
            .collect::<Vec<_>>()
            .join("\n");
        let items = vec![
            item(0, Some("garden"), "small"),
            item(1, Some("garden"), &long_body),
        ];
        let chunks = chunk_items_for_llm(&items, 120);
        assert_eq!(chunks[0].item_indices, vec![0]);
        let oversized: Vec<_> = chunks
            .iter()
            .filter(|chunk| chunk.item_indices == vec![1])
            .collect();
        assert!(oversized.len() >= 2, "long item split on line boundaries");
        assert!(oversized[0].warnings[0].contains("exceeds the chunk size"));
        let rejoined: String = oversized
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(rejoined, long_body);
    }

    #[test]
    fn zero_max_falls_back_to_the_default() {
        let items = vec![item(0, None, "body")];
        let chunks = chunk_items_for_llm(&items, 0);
        assert_eq!(chunks.len(), 1);
    }
}
