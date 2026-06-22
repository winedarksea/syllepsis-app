//! The text→coordinate lookup table (object-types.md "Location Metadata", spatial-worlds.md
//! "Worlds registry"). A small CSV mapping a place name to a world and a coordinate pair, so a
//! plain-text location (`@kitchen`, "job site") keeps working and simply resolves within its
//! world.
//!
//! Stored as CSV (not frontmatter) because it is a flat table users may also edit in a
//! spreadsheet. Columns: `name,world,first,second`. `first`/`second` are an uninterpreted pair
//! (lat/long for geo worlds, normalized x/y for image worlds) — the world's kind decides their
//! meaning at [resolve](crate::spatial::resolve) time, exactly as for inline `loc:` pairs.
//!
//! Names are matched case-insensitively and must not contain commas (the CSV field separator).

use crate::error::{CoreError, CoreResult};
use crate::model::DEFAULT_WORLD_ID;

const CSV_HEADER: &str = "name,world,first,second";

/// One row of the lookup table.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LookupEntry {
    /// The place name (e.g. `kitchen`, `the job site`). Matched case-insensitively.
    pub name: String,
    /// The world this place lives in (defaults to `earth`).
    pub world: String,
    /// First coordinate component: latitude for geo worlds, normalized `x` for image worlds.
    pub first: f64,
    /// Second coordinate component: longitude for geo worlds, normalized `y` for image worlds.
    pub second: f64,
}

impl LookupEntry {
    pub fn new(name: impl Into<String>, world: impl Into<String>, first: f64, second: f64) -> Self {
        LookupEntry {
            name: name.into(),
            world: world.into(),
            first,
            second,
        }
    }
}

/// The whole table, indexed by case-insensitive name (within a world).
#[derive(Debug, Clone, Default)]
pub struct LocationLookup {
    entries: Vec<LookupEntry>,
}

impl LocationLookup {
    pub fn new() -> LocationLookup {
        LocationLookup::default()
    }

    /// Parse the table from CSV text, tolerating a missing/blank file (→ empty table). The header
    /// row is optional; a first row that isn't the header is treated as data.
    pub fn from_csv(text: &str) -> CoreResult<LocationLookup> {
        let mut entries = Vec::new();
        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line == CSV_HEADER {
                continue;
            }
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() != 4 {
                return Err(CoreError::parse(
                    "location lookup csv",
                    format!(
                        "line {}: expected 4 columns, got {}",
                        line_no + 1,
                        fields.len()
                    ),
                ));
            }
            let first = fields[2].parse::<f64>().map_err(|_| {
                CoreError::parse(
                    "location lookup csv",
                    format!("line {}: invalid number '{}'", line_no + 1, fields[2]),
                )
            })?;
            let second = fields[3].parse::<f64>().map_err(|_| {
                CoreError::parse(
                    "location lookup csv",
                    format!("line {}: invalid number '{}'", line_no + 1, fields[3]),
                )
            })?;
            let world = if fields[1].is_empty() {
                DEFAULT_WORLD_ID.to_string()
            } else {
                fields[1].to_string()
            };
            entries.push(LookupEntry::new(fields[0], world, first, second));
        }
        Ok(LocationLookup { entries })
    }

    /// Serialize the table back to CSV (header included).
    pub fn to_csv(&self) -> String {
        let mut out = String::from(CSV_HEADER);
        out.push('\n');
        for e in &self.entries {
            out.push_str(&format!(
                "{},{},{},{}\n",
                e.name, e.world, e.first, e.second
            ));
        }
        out
    }

    /// Resolve a place name to its entry. When `world_hint` is given, an entry in that world is
    /// preferred; otherwise the first case-insensitive name match wins.
    pub fn resolve(&self, name: &str, world_hint: Option<&str>) -> Option<&LookupEntry> {
        let name_lc = name.to_lowercase();
        let matches = || {
            self.entries
                .iter()
                .filter(|e| e.name.to_lowercase() == name_lc)
        };
        if let Some(world) = world_hint {
            if let Some(hit) = matches().find(|e| e.world == world) {
                return Some(hit);
            }
        }
        matches().next()
    }

    /// Insert or replace an entry, keyed on (case-insensitive name, world).
    pub fn upsert(&mut self, entry: LookupEntry) {
        let name_lc = entry.name.to_lowercase();
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.name.to_lowercase() == name_lc && e.world == entry.world)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn entries(&self) -> &[LookupEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_csv() {
        let mut t = LocationLookup::new();
        t.upsert(LookupEntry::new("kitchen", "firstfloor", 0.42, 0.31));
        t.upsert(LookupEntry::new("job site", "earth", 47.6, -122.3));
        let csv = t.to_csv();
        assert!(csv.starts_with(CSV_HEADER));
        let back = LocationLookup::from_csv(&csv).unwrap();
        assert_eq!(back.entries(), t.entries());
    }

    #[test]
    fn resolve_is_case_insensitive_and_world_aware() {
        let mut t = LocationLookup::new();
        t.upsert(LookupEntry::new("Kitchen", "firstfloor", 0.4, 0.3));
        t.upsert(LookupEntry::new("kitchen", "secondfloor", 0.6, 0.7));
        // Case-insensitive name match.
        assert!(t.resolve("KITCHEN", None).is_some());
        // World hint disambiguates duplicate names across worlds.
        assert_eq!(
            t.resolve("kitchen", Some("secondfloor")).unwrap().second,
            0.7
        );
    }

    #[test]
    fn upsert_replaces_same_name_and_world() {
        let mut t = LocationLookup::new();
        t.upsert(LookupEntry::new("kitchen", "firstfloor", 0.1, 0.1));
        t.upsert(LookupEntry::new("kitchen", "firstfloor", 0.9, 0.9));
        assert_eq!(t.entries().len(), 1);
        assert_eq!(t.entries()[0].first, 0.9);
    }

    #[test]
    fn empty_or_header_only_csv_is_empty() {
        assert!(LocationLookup::from_csv("").unwrap().entries().is_empty());
        assert!(LocationLookup::from_csv(CSV_HEADER)
            .unwrap()
            .entries()
            .is_empty());
    }

    #[test]
    fn blank_world_defaults_to_earth() {
        let t = LocationLookup::from_csv("home,,47.6,-122.3").unwrap();
        assert_eq!(t.entries()[0].world, DEFAULT_WORLD_ID);
    }
}
