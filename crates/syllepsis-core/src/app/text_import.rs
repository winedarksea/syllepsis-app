//! Deterministic long-text import preview and commit.
//!
//! This deliberately stays below the LLM layer: the importer must preserve source text and make
//! every split reviewable before it writes notes.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::{CoreError, CoreResult};
use crate::id::{slugify, NoteId};
use crate::markdown::dialect;
use crate::model::{Category, Note, ObjectType, PriorEdge, PriorKind};
use crate::storage::{Book, NoteStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextImportSplitMode {
    OneNote,
    NonEmptyLine,
    Paragraph,
    Smart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportOptions {
    pub split_mode: TextImportSplitMode,
    pub detect_headings: bool,
    pub detect_lists: bool,
    pub detect_tables: bool,
    pub detect_code_blocks: bool,
    pub convert_indented_lists: bool,
}

impl Default for TextImportOptions {
    fn default() -> Self {
        TextImportOptions {
            split_mode: TextImportSplitMode::Smart,
            detect_headings: true,
            detect_lists: true,
            detect_tables: true,
            detect_code_blocks: true,
            convert_indented_lists: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextImportBlockKind {
    Paragraph,
    List,
    Table,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextImportPriorPreviewTarget {
    None,
    PreviousImportedNote,
    Category,
    ExistingNote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportPriorPreview {
    pub target: TextImportPriorPreviewTarget,
    pub target_label: Option<String>,
    pub kind: PriorKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportPreviewItem {
    pub index: usize,
    pub title: String,
    pub body: String,
    pub block_kind: TextImportBlockKind,
    pub category_context: Option<String>,
    pub intended_prior: Option<TextImportPriorPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportCategoryPreview {
    pub name: String,
    pub long_name: String,
    pub heading_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportPreview {
    pub items: Vec<TextImportPreviewItem>,
    pub categories: Vec<TextImportCategoryPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextImportPlacement {
    Unsorted,
    Category { category: String },
    AfterNote { note_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportCommitRequest {
    pub items: Vec<TextImportPreviewItem>,
    pub categories: Vec<TextImportCategoryPreview>,
    pub placement: TextImportPlacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportReport {
    pub imported: Vec<String>,
    pub created_categories: Vec<String>,
    pub first_note_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedBlock {
    title: String,
    body: String,
    kind: TextImportBlockKind,
    category_context: Option<String>,
    prior_kind: PriorKind,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct HeadingContext {
    name: String,
}

pub fn preview_text_import(source_text: &str, options: &TextImportOptions) -> TextImportPreview {
    let normalized = source_text.replace("\r\n", "\n").replace('\r', "\n");
    let (blocks, categories, warnings) = match options.split_mode {
        TextImportSplitMode::OneNote => parse_one_note(&normalized, options),
        TextImportSplitMode::NonEmptyLine => parse_non_empty_lines(&normalized, options),
        TextImportSplitMode::Paragraph => parse_paragraphs(&normalized, options),
        TextImportSplitMode::Smart => parse_smart_blocks(&normalized, options),
    };

    let items = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            let target = if index == 0 {
                block
                    .category_context
                    .as_ref()
                    .map(|category| TextImportPriorPreview {
                        target: TextImportPriorPreviewTarget::Category,
                        target_label: Some(category.clone()),
                        kind: PriorKind::NewParagraph,
                    })
            } else {
                Some(TextImportPriorPreview {
                    target: TextImportPriorPreviewTarget::PreviousImportedNote,
                    target_label: None,
                    kind: block.prior_kind,
                })
            };
            TextImportPreviewItem {
                index,
                title: block.title,
                body: block.body,
                block_kind: block.kind,
                category_context: block.category_context,
                intended_prior: target,
                warnings: block.warnings,
            }
        })
        .collect();

    TextImportPreview {
        items,
        categories,
        warnings,
    }
}

pub fn commit_text_import(
    book: &Book,
    request: TextImportCommitRequest,
) -> CoreResult<TextImportReport> {
    if request.items.is_empty() {
        return Err(CoreError::InvalidBook(
            "text import has no preview items to commit".to_string(),
        ));
    }

    let mut created_categories = Vec::new();
    ensure_preview_categories(book, &request.categories, &mut created_categories)?;
    if let TextImportPlacement::Category { category } = &request.placement {
        ensure_category_exists(book, category, category, 2, &mut created_categories)?;
    }

    let mut imported = Vec::new();
    let mut previous_note_id: Option<NoteId> = None;
    let mut active_category = placement_start_category(&request.placement);
    let after_note_id = match &request.placement {
        TextImportPlacement::AfterNote { note_id } => Some(NoteId::parse(note_id)?),
        _ => None,
    };

    for (index, item) in request.items.iter().enumerate() {
        if item.body.trim().is_empty() {
            continue;
        }
        if let Some(category) = &item.category_context {
            active_category = Some(category.clone());
            ensure_category_exists(book, category, category, 2, &mut created_categories)?;
        }

        let mut note = Note::new(
            ObjectType::Note,
            item.title.trim(),
            book.config.markdown.dialect_version.clone(),
        );
        note.body = item.body.clone();
        note.categories = categories_for_item(item);
        for category in &note.categories {
            ensure_category_exists(book, category, category, 2, &mut created_categories)?;
        }
        note.metadata.dates.updated = Utc::now();
        note.metadata.dates.created = note.metadata.dates.updated;

        note.prior = if let Some(previous) = previous_note_id.clone() {
            Some(PriorEdge::follows(previous, intended_kind(item)))
        } else if index == 0 {
            match &request.placement {
                TextImportPlacement::Unsorted => {
                    active_category.clone().map(PriorEdge::starts_category)
                }
                TextImportPlacement::Category { category } => {
                    Some(PriorEdge::starts_category(category.clone()))
                }
                TextImportPlacement::AfterNote { .. } => after_note_id
                    .clone()
                    .map(|id| PriorEdge::follows(id, intended_kind(item))),
            }
        } else {
            active_category.clone().map(PriorEdge::starts_category)
        };

        book.save_note(&note)?;
        previous_note_id = Some(note.id.clone());
        imported.push(note.id.to_string());
    }

    let first_note_id = imported.first().cloned();
    Ok(TextImportReport {
        imported,
        created_categories,
        first_note_id,
    })
}

fn parse_one_note(
    text: &str,
    options: &TextImportOptions,
) -> (
    Vec<ParsedBlock>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    let mut categories = Vec::new();
    let heading = first_heading(text).filter(|_| options.detect_headings);
    if let Some((level, long_name)) = heading.clone() {
        categories.push(category_preview(&long_name, level));
    }
    let title = heading
        .map(|(_, heading)| heading)
        .or_else(|| first_content_line(text))
        .unwrap_or_else(|| "Imported text".to_string());
    let block = ParsedBlock {
        title: title_from_text(&title),
        body: maybe_convert_indented_lists(text.trim(), options),
        kind: TextImportBlockKind::Paragraph,
        category_context: None,
        prior_kind: PriorKind::NewParagraph,
        warnings: Vec::new(),
    };
    (non_empty_blocks(vec![block]), categories, Vec::new())
}

fn parse_non_empty_lines(
    text: &str,
    options: &TextImportOptions,
) -> (
    Vec<ParsedBlock>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    let mut categories = Vec::new();
    let mut current_heading: Option<HeadingContext> = None;
    let mut blocks = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if options.detect_headings {
            if let Some(heading) = parse_heading(trimmed) {
                current_heading = Some(heading_context(heading.0, &heading.1));
                categories.push(category_preview(&heading.1, heading.0));
                continue;
            }
        }
        let body = maybe_convert_indented_lists(trimmed, options);
        blocks.push(ParsedBlock {
            title: title_from_text(trimmed),
            body,
            kind: line_kind(trimmed, options),
            category_context: current_heading.as_ref().map(|h| h.name.clone()),
            prior_kind: line_prior_kind(trimmed, options),
            warnings: Vec::new(),
        });
    }

    (blocks, dedupe_categories(categories), Vec::new())
}

fn parse_paragraphs(
    text: &str,
    options: &TextImportOptions,
) -> (
    Vec<ParsedBlock>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    parse_blocks(text, options, false)
}

fn parse_smart_blocks(
    text: &str,
    options: &TextImportOptions,
) -> (
    Vec<ParsedBlock>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    parse_blocks(text, options, true)
}

fn parse_blocks(
    text: &str,
    options: &TextImportOptions,
    split_list_items: bool,
) -> (
    Vec<ParsedBlock>,
    Vec<TextImportCategoryPreview>,
    Vec<String>,
) {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut categories = Vec::new();
    let mut warnings = Vec::new();
    let mut current_heading: Option<HeadingContext> = None;
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        if options.detect_headings {
            if let Some((level, heading)) = parse_heading(lines[i].trim()) {
                current_heading = Some(heading_context(level, &heading));
                categories.push(category_preview(&heading, level));
                i += 1;
                continue;
            }
        }

        if options.detect_code_blocks && is_fence_start(lines[i]) {
            let (body, next, closed) = collect_code_block(&lines, i);
            let mut block_warnings = Vec::new();
            if !closed {
                block_warnings
                    .push("Code fence was not closed; imported through end of text.".to_string());
            }
            blocks.push(block_from_body(
                body,
                TextImportBlockKind::Code,
                current_heading.as_ref(),
                PriorKind::NewParagraph,
                block_warnings,
            ));
            i = next;
            continue;
        }

        if options.detect_tables && is_table_start(&lines, i) {
            let (body, next) = collect_while(&lines, i, |line| {
                line.trim().contains('|') && !line.trim().is_empty()
            });
            blocks.push(block_from_body(
                body,
                TextImportBlockKind::Table,
                current_heading.as_ref(),
                PriorKind::NewParagraph,
                Vec::new(),
            ));
            i = next;
            continue;
        }

        if options.detect_lists && is_list_line(lines[i], options.convert_indented_lists) {
            let (body, next) = collect_while(&lines, i, |line| {
                line.trim().is_empty() || is_list_line(line, options.convert_indented_lists)
            });
            if split_list_items {
                blocks.extend(split_list_block(&body, options, current_heading.as_ref()));
            } else {
                blocks.push(block_from_body(
                    maybe_convert_indented_lists(&body, options),
                    TextImportBlockKind::List,
                    current_heading.as_ref(),
                    PriorKind::NewParagraph,
                    Vec::new(),
                ));
            }
            i = next;
            continue;
        }

        let (body, next) = collect_paragraph(&lines, i, options);
        if body.trim().contains('|') && !options.detect_tables {
            warnings.push(
                "Table-looking text was imported as a paragraph because table detection is off."
                    .to_string(),
            );
        }
        blocks.push(block_from_body(
            maybe_convert_indented_lists(&body, options),
            TextImportBlockKind::Paragraph,
            current_heading.as_ref(),
            PriorKind::NewParagraph,
            Vec::new(),
        ));
        i = next;
    }

    (
        non_empty_blocks(blocks),
        dedupe_categories(categories),
        warnings,
    )
}

fn block_from_body(
    body: String,
    kind: TextImportBlockKind,
    heading: Option<&HeadingContext>,
    prior_kind: PriorKind,
    warnings: Vec<String>,
) -> ParsedBlock {
    ParsedBlock {
        title: title_from_text(&body),
        body,
        kind,
        category_context: heading.map(|h| h.name.clone()),
        prior_kind,
        warnings,
    }
}

fn split_list_block(
    body: &str,
    options: &TextImportOptions,
    heading: Option<&HeadingContext>,
) -> Vec<ParsedBlock> {
    let normalized = maybe_convert_indented_lists(body, options);
    let mut blocks = Vec::new();
    for line in normalized.lines().filter(|line| !line.trim().is_empty()) {
        let kind = if is_numbered_list_line(line) {
            PriorKind::NumberedList
        } else {
            PriorKind::BulletPoint
        };
        blocks.push(block_from_body(
            line.trim().to_string(),
            TextImportBlockKind::List,
            heading,
            kind,
            Vec::new(),
        ));
    }
    blocks
}

fn collect_code_block(lines: &[&str], start: usize) -> (String, usize, bool) {
    let Some(fence) = fence_marker(lines[start]) else {
        return (lines[start].to_string(), start + 1, false);
    };
    let mut body = Vec::new();
    body.push(lines[start]);
    let mut i = start + 1;
    while i < lines.len() {
        body.push(lines[i]);
        if fence_closes(lines[i], fence) {
            return (body.join("\n"), i + 1, true);
        }
        i += 1;
    }
    (body.join("\n"), lines.len(), false)
}

fn collect_paragraph(lines: &[&str], start: usize, options: &TextImportOptions) -> (String, usize) {
    let mut body = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty()
            || (options.detect_headings && parse_heading(trimmed).is_some())
            || (options.detect_code_blocks && is_fence_start(lines[i]))
            || (options.detect_tables && is_table_start(lines, i))
            || (options.detect_lists && is_list_line(lines[i], options.convert_indented_lists))
        {
            break;
        }
        body.push(lines[i]);
        i += 1;
    }
    (body.join("\n"), i)
}

fn collect_while<F>(lines: &[&str], start: usize, predicate: F) -> (String, usize)
where
    F: Fn(&str) -> bool,
{
    let mut body = Vec::new();
    let mut i = start;
    while i < lines.len() && predicate(lines[i]) {
        body.push(lines[i]);
        i += 1;
    }
    (body.join("\n"), i)
}

fn non_empty_blocks(blocks: Vec<ParsedBlock>) -> Vec<ParsedBlock> {
    blocks
        .into_iter()
        .filter(|block| !block.body.trim().is_empty())
        .collect()
}

fn parse_heading(line: &str) -> Option<(u8, String)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = line.get(hashes..)?.trim();
    if rest.is_empty()
        || !line
            .as_bytes()
            .get(hashes)
            .is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    Some((hashes as u8, rest.trim_matches('#').trim().to_string()))
}

fn first_heading(text: &str) -> Option<(u8, String)> {
    text.lines().find_map(|line| parse_heading(line.trim()))
}

fn first_content_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn title_from_text(text: &str) -> String {
    let first = first_content_line(text).unwrap_or_else(|| "Imported text".to_string());
    let stripped = strip_markdown_prefix(&first);
    let compact = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = compact.chars().take(72).collect();
    if compact.chars().count() > 72 {
        title.push_str("...");
    }
    if title.is_empty() {
        "Imported text".to_string()
    } else {
        title
    }
}

fn strip_markdown_prefix(line: &str) -> String {
    let trimmed = line.trim();
    if let Some((_, heading)) = parse_heading(trimmed) {
        return heading;
    }
    if let Some(rest) = strip_bullet_marker(trimmed) {
        return rest.to_string();
    }
    if let Some(rest) = strip_numbered_marker(trimmed) {
        return rest.to_string();
    }
    trimmed.trim_matches('`').to_string()
}

fn line_kind(line: &str, options: &TextImportOptions) -> TextImportBlockKind {
    if options.detect_tables && line.contains('|') {
        TextImportBlockKind::Table
    } else if options.detect_lists && is_list_line(line, options.convert_indented_lists) {
        TextImportBlockKind::List
    } else if options.detect_code_blocks && is_fence_start(line) {
        TextImportBlockKind::Code
    } else {
        TextImportBlockKind::Paragraph
    }
}

fn line_prior_kind(line: &str, options: &TextImportOptions) -> PriorKind {
    if options.detect_lists && is_numbered_list_line(line) {
        PriorKind::NumberedList
    } else if options.detect_lists && is_list_line(line, options.convert_indented_lists) {
        PriorKind::BulletPoint
    } else {
        PriorKind::NewParagraph
    }
}

fn is_fence_start(line: &str) -> bool {
    fence_marker(line).is_some()
}

#[derive(Debug, Clone, Copy)]
struct FenceMarker {
    ch: char,
    len: usize,
}

fn fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    if len < 3 {
        return None;
    }
    if ch == '`' || ch == '~' {
        Some(FenceMarker { ch, len })
    } else {
        None
    }
}

fn fence_closes(line: &str, opener: FenceMarker) -> bool {
    let trimmed = line.trim_start();
    let close_len = trimmed.chars().take_while(|c| *c == opener.ch).count();
    if close_len < opener.len {
        return false;
    }
    trimmed
        .chars()
        .skip(close_len)
        .all(|ch| ch.is_ascii_whitespace())
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    let header = lines[index].trim();
    let separator = lines[index + 1].trim();
    header.contains('|') && is_table_separator(separator)
}

fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim_matches('|').trim();
    !trimmed.is_empty()
        && trimmed.split('|').all(|cell| {
            let cell = cell.trim();
            cell.len() >= 3
                && cell
                    .chars()
                    .all(|c| c == '-' || c == ':' || c.is_whitespace())
        })
}

fn is_list_line(line: &str, allow_indented_outline: bool) -> bool {
    let trimmed = line.trim_start();
    strip_bullet_marker(trimmed).is_some()
        || strip_numbered_marker(trimmed).is_some()
        || (allow_indented_outline && is_indented_outline_line(line))
}

fn is_numbered_list_line(line: &str) -> bool {
    strip_numbered_marker(line.trim_start()).is_some()
}

fn strip_bullet_marker(line: &str) -> Option<&str> {
    let mut chars = line.chars();
    let marker = chars.next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }
    let rest = chars.as_str();
    rest.chars()
        .next()
        .filter(|c| c.is_whitespace())
        .map(|_| rest.trim())
}

fn strip_numbered_marker(line: &str) -> Option<&str> {
    let digit_count = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let rest = line.get(digit_count..)?;
    let rest = rest.strip_prefix('.')?;
    rest.chars()
        .next()
        .filter(|c| c.is_whitespace())
        .map(|_| rest.trim())
}

fn is_indented_outline_line(line: &str) -> bool {
    let leading_spaces = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    leading_spaces > 0 && !line.trim().is_empty()
}

fn maybe_convert_indented_lists(text: &str, options: &TextImportOptions) -> String {
    if !options.convert_indented_lists {
        return text.to_string();
    }
    text.lines()
        .map(|line| {
            if is_indented_outline_line(line) && !is_list_line(line, false) {
                let indent = line
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect::<String>();
                format!("{indent}- {}", line.trim())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn heading_context(level: u8, long_name: &str) -> HeadingContext {
    let preview = category_preview(long_name, level);
    HeadingContext { name: preview.name }
}

fn category_preview(long_name: &str, level: u8) -> TextImportCategoryPreview {
    let mut name = slugify(long_name);
    if name.is_empty() {
        name = "imported-section".to_string();
    }
    TextImportCategoryPreview {
        name,
        long_name: long_name.trim().to_string(),
        heading_level: level.clamp(1, 6),
    }
}

fn dedupe_categories(categories: Vec<TextImportCategoryPreview>) -> Vec<TextImportCategoryPreview> {
    let mut seen = HashSet::new();
    categories
        .into_iter()
        .filter(|category| seen.insert(category.name.clone()))
        .collect()
}

fn ensure_preview_categories(
    book: &Book,
    categories: &[TextImportCategoryPreview],
    created_categories: &mut Vec<String>,
) -> CoreResult<()> {
    for category in categories {
        ensure_category_exists(
            book,
            &category.name,
            &category.long_name,
            category.heading_level,
            created_categories,
        )?;
    }
    Ok(())
}

fn ensure_category_exists(
    book: &Book,
    name: &str,
    long_name: &str,
    heading_level: u8,
    created_categories: &mut Vec<String>,
) -> CoreResult<()> {
    if book.store.read_category(name).is_ok() {
        return Ok(());
    }
    let mut category = Category::new(name.to_string());
    category.long_name = if long_name.trim().is_empty() {
        name.to_string()
    } else {
        long_name.trim().to_string()
    };
    category.heading_level = heading_level.clamp(1, 6);
    book.store.write_category(&category)?;
    if !created_categories.contains(&category.name) {
        created_categories.push(category.name);
    }
    Ok(())
}

fn placement_start_category(placement: &TextImportPlacement) -> Option<String> {
    match placement {
        TextImportPlacement::Category { category } => Some(category.clone()),
        _ => None,
    }
}

fn intended_kind(item: &TextImportPreviewItem) -> PriorKind {
    item.intended_prior
        .as_ref()
        .map(|prior| prior.kind)
        .unwrap_or(PriorKind::NewParagraph)
}

fn categories_for_item(item: &TextImportPreviewItem) -> Vec<String> {
    let mut categories = Vec::new();
    if let Some(category) = &item.category_context {
        categories.push(category.clone());
    }
    for category in dialect::extract_categories(&item.body) {
        if !categories.contains(&category) {
            categories.push(category);
        }
    }
    categories
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PriorRef;
    use tempfile::tempdir;

    fn defaults(mode: TextImportSplitMode) -> TextImportOptions {
        TextImportOptions {
            split_mode: mode,
            ..TextImportOptions::default()
        }
    }

    #[test]
    fn previews_one_note_import() {
        let preview = preview_text_import(
            "Title\n\nBody paragraph.",
            &defaults(TextImportSplitMode::OneNote),
        );
        assert_eq!(preview.items.len(), 1);
        assert_eq!(preview.items[0].title, "Title");
        assert!(preview.items[0].body.contains("Body paragraph."));
    }

    #[test]
    fn splits_non_empty_lines() {
        let preview =
            preview_text_import("a\n\nb\nc", &defaults(TextImportSplitMode::NonEmptyLine));
        assert_eq!(preview.items.len(), 3);
        assert_eq!(preview.items[1].body, "b");
    }

    #[test]
    fn paragraph_split_keeps_code_fence_intact() {
        let input = "First.\n\n```rust\nfn main() {}\n```\n\nLast.";
        let preview = preview_text_import(input, &defaults(TextImportSplitMode::Paragraph));
        assert_eq!(preview.items.len(), 3);
        assert_eq!(preview.items[1].block_kind, TextImportBlockKind::Code);
        assert!(preview.items[1].body.contains("fn main"));
    }

    #[test]
    fn paragraph_split_respects_long_outer_code_fence() {
        let input = "Before.\n\n````\n# Notes\n\n```python\nprint('nested')\n```\n````\n\nAfter.";
        let preview = preview_text_import(input, &defaults(TextImportSplitMode::Paragraph));
        assert_eq!(preview.items.len(), 3);
        assert_eq!(preview.items[1].block_kind, TextImportBlockKind::Code);
        assert!(preview.items[1].body.contains("```python"));
        assert!(preview.items[1].body.contains("````"));
        assert_eq!(preview.items[2].body, "After.");
    }

    #[test]
    fn paragraph_split_does_not_close_tilde_fence_on_backticks() {
        let input = "~~~~\n```python\nprint('nested')\n```\n~~~~\n\nAfter.";
        let preview = preview_text_import(input, &defaults(TextImportSplitMode::Paragraph));
        assert_eq!(preview.items.len(), 2);
        assert_eq!(preview.items[0].block_kind, TextImportBlockKind::Code);
        assert!(preview.items[0].body.contains("```python"));
        assert_eq!(preview.items[1].body, "After.");
    }

    #[test]
    fn smart_split_detects_headings_tables_and_lists() {
        let input = "# Chapter\n\nIntro.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n- one\n- two";
        let preview = preview_text_import(input, &defaults(TextImportSplitMode::Smart));
        assert_eq!(preview.categories[0].name, "chapter");
        assert_eq!(
            preview.items[0].category_context.as_deref(),
            Some("chapter")
        );
        assert_eq!(preview.items[1].block_kind, TextImportBlockKind::Table);
        assert_eq!(preview.items[2].block_kind, TextImportBlockKind::List);
        assert_eq!(
            preview.items[2].intended_prior.as_ref().unwrap().kind,
            PriorKind::BulletPoint
        );
    }

    #[test]
    fn converts_indented_outline_lines_to_bullets() {
        let mut options = defaults(TextImportSplitMode::Smart);
        options.convert_indented_lists = true;
        let preview = preview_text_import("Parent\n  child\n  second", &options);
        assert!(preview
            .items
            .iter()
            .any(|item| item.body.contains("- child")));
    }

    #[test]
    fn commit_chains_imported_notes_in_order() {
        let dir = tempdir().unwrap();
        let book = Book::create(dir.path().join("book"), "Book").unwrap();
        let preview = preview_text_import(
            "# Section\n\nOne.\n\nTwo.",
            &defaults(TextImportSplitMode::Smart),
        );
        let report = commit_text_import(
            &book,
            TextImportCommitRequest {
                items: preview.items,
                categories: preview.categories,
                placement: TextImportPlacement::Unsorted,
            },
        )
        .unwrap();
        assert_eq!(report.imported.len(), 2);
        assert_eq!(report.created_categories, vec!["section".to_string()]);
        let first = book
            .store
            .read_note(&NoteId::parse(&report.imported[0]).unwrap())
            .unwrap();
        let second = book
            .store
            .read_note(&NoteId::parse(&report.imported[1]).unwrap())
            .unwrap();
        assert!(matches!(
            first.prior.as_ref().unwrap().target,
            PriorRef::Category(ref name) if name == "section"
        ));
        assert!(matches!(
            second.prior.as_ref().unwrap().target,
            PriorRef::Note(ref id) if id == &first.id
        ));
    }
}
