//! Detect and map Obsidian-style YAML frontmatter at the top of imported text.
//!
//! The text importer treats its input as plain prose, so a leading `---` frontmatter block would
//! otherwise become a garbage note or pollute the first note's body. This module strips that block
//! before the importer splits the text and maps its recognized fields (created/updated dates, tags,
//! aliases, status) onto every note produced from the file. Unrecognized keys are dropped silently.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::id::slugify;
use crate::markdown::frontmatter::split_frontmatter;
use crate::model::NoteStatus;

/// Frontmatter fields the importer understands, mapped onto every note split from the file.
/// Serialized across the Tauri boundary so the preview can show what will be applied and the
/// commit request can carry it back unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextImportFrontmatter {
    pub created: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    /// Slugified so they can be applied directly as note categories.
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub status: Option<NoteStatus>,
}

/// Lenient parse target: unknown keys are ignored (`#[serde(deny_unknown_fields)]` is deliberately
/// absent) so unmapped Obsidian keys are stripped silently. Every field is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFrontmatter {
    #[serde(alias = "date created")]
    created: Option<serde_yaml::Value>,
    #[serde(alias = "modified", alias = "last modified")]
    updated: Option<serde_yaml::Value>,
    #[serde(alias = "tag")]
    tags: Option<serde_yaml::Value>,
    #[serde(alias = "alias")]
    aliases: Option<serde_yaml::Value>,
    status: Option<serde_yaml::Value>,
}

/// Strip a leading YAML frontmatter block and map its fields.
///
/// Returns the mapped frontmatter (when present and parseable), the body with the frontmatter
/// removed, and any warnings to surface in the preview. When the text does not open with a `---`
/// fence, or the YAML fails to parse, the original text is returned untouched.
pub fn extract_frontmatter(text: &str) -> (Option<TextImportFrontmatter>, String, Vec<String>) {
    let Some((yaml, body)) = split_frontmatter(text) else {
        return (None, text.to_string(), Vec::new());
    };

    let raw: RawFrontmatter = match serde_yaml::from_str(&yaml) {
        Ok(raw) => raw,
        Err(_) => {
            return (
                None,
                text.to_string(),
                vec![
                    "Frontmatter could not be parsed as YAML; imported as plain text.".to_string(),
                ],
            );
        }
    };

    let mut warnings = Vec::new();
    let created = parse_date_field(raw.created, "created", &mut warnings);
    let updated = parse_date_field(raw.updated, "updated", &mut warnings);
    let tags = parse_string_list(raw.tags)
        .into_iter()
        .map(|tag| slugify(&tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    let aliases = parse_string_list(raw.aliases)
        .into_iter()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .collect();
    let status = raw.status.and_then(|value| {
        let text = yaml_scalar_string(&value)?;
        match map_status(&text) {
            Some(status) => Some(status),
            None => {
                warnings.push(format!(
                    "Frontmatter status \"{}\" was not recognized and was ignored.",
                    text.trim()
                ));
                None
            }
        }
    });

    let fm = TextImportFrontmatter {
        created,
        updated,
        tags,
        aliases,
        status,
    };
    (Some(fm), body, warnings)
}

/// Parse a YAML value into a UTC datetime, accepting the forms Obsidian and its Linter plugin emit.
fn parse_date_field(
    value: Option<serde_yaml::Value>,
    field: &str,
    warnings: &mut Vec<String>,
) -> Option<DateTime<Utc>> {
    let value = value?;
    // A native YAML timestamp deserializes straight into a UTC datetime.
    if let Ok(dt) = serde_yaml::from_value::<DateTime<Utc>>(value.clone()) {
        return Some(dt);
    }
    let text = yaml_scalar_string(&value)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    match parse_date_string(trimmed) {
        Some(dt) => Some(dt),
        None => {
            warnings.push(format!(
                "Frontmatter {field} date \"{trimmed}\" could not be parsed and was ignored."
            ));
            None
        }
    }
}

/// RFC 3339 → `%Y-%m-%d %H:%M(:%S)?` (Obsidian Linter default, treated as UTC) → bare `%Y-%m-%d`.
fn parse_date_string(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?));
    }
    None
}

/// Coerce a `tags`/`aliases` value into a list of strings: a YAML sequence yields one entry per
/// item, a single string is split on commas (both forms occur in real Obsidian vaults).
fn parse_string_list(value: Option<serde_yaml::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(yaml_scalar_string)
            .flat_map(|item| split_comma_list(&item))
            .collect(),
        Some(other) => yaml_scalar_string(&other)
            .map(|text| split_comma_list(&text))
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

fn split_comma_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// Render a scalar YAML value (string, number, bool) as a string; sequences/maps yield `None`.
fn yaml_scalar_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Fuzzy-map a frontmatter status string to a [`NoteStatus`]. Case-, and `-`/`_`/space-insensitive.
fn map_status(raw: &str) -> Option<NoteStatus> {
    let key: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    match key.as_str() {
        "todo" | "open" => Some(NoteStatus::Open),
        "inprogress" | "doing" | "active" | "wip" => Some(NoteStatus::Active),
        "needsclarification" | "question" => Some(NoteStatus::NeedsClarification),
        "onhold" | "waiting" | "deferred" | "blocked" | "someday" => Some(NoteStatus::Deferred),
        "cancelled" | "canceled" | "dropped" | "wontdo" => Some(NoteStatus::Cancelled),
        "done" | "complete" | "completed" | "finished" => Some(NoteStatus::Done),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_full_obsidian_frontmatter_and_strips_fence() {
        let input = "---\ncreated: 2023-05-01T10:00:00Z\nupdated: 2023-06-02\ntags:\n  - project/alpha\n  - idea\naliases:\n  - Alt Name\nstatus: in-progress\n---\nBody paragraph.\n";
        let (fm, body, warnings) = extract_frontmatter(input);
        let fm = fm.unwrap();
        assert!(warnings.is_empty());
        assert_eq!(
            fm.created,
            Some(Utc.with_ymd_and_hms(2023, 5, 1, 10, 0, 0).unwrap())
        );
        assert_eq!(
            fm.updated,
            Some(Utc.with_ymd_and_hms(2023, 6, 2, 0, 0, 0).unwrap())
        );
        assert_eq!(fm.tags, vec!["project-alpha", "idea"]);
        assert_eq!(fm.aliases, vec!["Alt Name"]);
        assert_eq!(fm.status, Some(NoteStatus::Active));
        assert!(!body.contains("---"));
        assert!(body.contains("Body paragraph."));
    }

    #[test]
    fn tags_accept_sequence_or_comma_string_and_slugify_nested() {
        let seq = extract_frontmatter("---\ntags:\n  - area/health\n  - Second Tag\n---\nx")
            .0
            .unwrap();
        assert_eq!(seq.tags, vec!["area-health", "second-tag"]);
        let csv = extract_frontmatter("---\ntag: alpha, beta/gamma\n---\nx")
            .0
            .unwrap();
        assert_eq!(csv.tags, vec!["alpha", "beta-gamma"]);
    }

    #[test]
    fn aliases_accept_sequence_or_single_string() {
        let seq = extract_frontmatter("---\naliases:\n  - One\n  - Two\n---\nx")
            .0
            .unwrap();
        assert_eq!(seq.aliases, vec!["One", "Two"]);
        let single = extract_frontmatter("---\nalias: Solo\n---\nx").0.unwrap();
        assert_eq!(single.aliases, vec!["Solo"]);
    }

    #[test]
    fn parses_date_forms_and_warns_on_garbage() {
        let rfc = extract_frontmatter("---\ncreated: 2024-01-02T13:45:00+02:00\n---\nx")
            .0
            .unwrap();
        assert_eq!(
            rfc.created,
            Some(Utc.with_ymd_and_hms(2024, 1, 2, 11, 45, 0).unwrap())
        );
        let linter = extract_frontmatter("---\ncreated: 2024-01-02 13:45\n---\nx")
            .0
            .unwrap();
        assert_eq!(
            linter.created,
            Some(Utc.with_ymd_and_hms(2024, 1, 2, 13, 45, 0).unwrap())
        );
        let bare = extract_frontmatter("---\ncreated: 2024-01-02\n---\nx")
            .0
            .unwrap();
        assert_eq!(
            bare.created,
            Some(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap())
        );
        let (fm, _, warnings) = extract_frontmatter("---\ncreated: not a date\n---\nx");
        assert_eq!(fm.unwrap().created, None);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn status_fuzzy_maps_and_warns_on_unknown() {
        for (raw, expected) in [
            ("todo", NoteStatus::Open),
            ("WIP", NoteStatus::Active),
            ("On Hold", NoteStatus::Deferred),
            ("wont-do", NoteStatus::Cancelled),
            ("Completed", NoteStatus::Done),
            ("needs_clarification", NoteStatus::NeedsClarification),
        ] {
            let fm = extract_frontmatter(&format!("---\nstatus: {raw}\n---\nx"))
                .0
                .unwrap();
            assert_eq!(fm.status, Some(expected), "status {raw}");
        }
        let (fm, _, warnings) = extract_frontmatter("---\nstatus: frozen\n---\nx");
        assert_eq!(fm.unwrap().status, None);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn passes_through_text_without_frontmatter() {
        let (fm, body, warnings) = extract_frontmatter("No fence here.\n\nMore text.");
        assert!(fm.is_none());
        assert_eq!(body, "No fence here.\n\nMore text.");
        assert!(warnings.is_empty());
    }

    #[test]
    fn horizontal_rule_mid_document_is_left_untouched() {
        let input = "Intro paragraph.\n\n---\n\nAfter the rule.";
        let (fm, body, warnings) = extract_frontmatter(input);
        assert!(fm.is_none());
        assert_eq!(body, input);
        assert!(warnings.is_empty());
    }

    #[test]
    fn malformed_yaml_preserves_text_and_warns() {
        let input = "---\ncreated: : :\n  bad: [unclosed\n---\nBody.";
        let (fm, body, warnings) = extract_frontmatter(input);
        assert!(fm.is_none());
        assert_eq!(body, input);
        assert_eq!(warnings.len(), 1);
    }
}
