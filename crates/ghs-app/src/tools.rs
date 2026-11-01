//! Application-owned tool table.
//!
//! The reference controller has no tool-table entries, so the app owns the
//! table and stores it in the per-controller profile. This module is a plain
//! data model: it validates records, keeps the table sorted, and tracks whether
//! a save is pending. It never writes controller data and never merges a
//! controller `[T:...]` report into the application table.

use std::path::{Path, PathBuf};

use ghs_config::{ToolRecord, ToolTable, ValidationError};

/// One editable numeric field in the tool sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolField {
    /// Tool number, the `T` word.
    Number,
    /// Tool diameter in millimetres.
    Diameter,
    /// Booking length in millimetres.
    Length,
    /// Optional pocket number.
    Pocket,
}

impl ToolField {
    /// Short title for the keypad.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            ToolField::Number => "TOOL NUMBER",
            ToolField::Diameter => "TOOL DIAMETER (MM)",
            ToolField::Length => "TOOL LENGTH (MM)",
            ToolField::Pocket => "POCKET NUMBER",
        }
    }

    /// All fields in sheet order.
    pub const ALL: [ToolField; 4] = [
        ToolField::Number,
        ToolField::Diameter,
        ToolField::Length,
        ToolField::Pocket,
    ];
}

/// Load/save state of the app-side tool table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolFileState {
    /// Profile file the table belongs to.
    pub path: Option<PathBuf>,
    /// Short human-readable status.
    pub status: String,
}

impl Default for ToolFileState {
    fn default() -> Self {
        Self {
            path: None,
            status: "NO PROFILE".to_string(),
        }
    }
}

/// The app-side tool table plus its persistence state.
#[derive(Clone, Debug, Default)]
pub struct ToolTableModel {
    table: ToolTable,
    dirty: bool,
    file: ToolFileState,
    error: Option<String>,
}

impl ToolTableModel {
    /// An empty table with no profile loaded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the table after a successful import or reload.
    pub fn adopt(&mut self, table: ToolTable, path: Option<PathBuf>, status: impl Into<String>) {
        self.table = table;
        self.dirty = false;
        self.error = None;
        self.file = ToolFileState {
            path,
            status: status.into(),
        };
    }

    /// Current table.
    #[must_use]
    pub fn table(&self) -> &ToolTable {
        &self.table
    }

    /// Find one tool by number.
    #[must_use]
    pub fn get(&self, number: u32) -> Option<&ToolRecord> {
        self.table.get(number)
    }

    /// Whether an edit is waiting for Save.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Profile path and status.
    #[must_use]
    pub fn file(&self) -> &ToolFileState {
        &self.file
    }

    /// Last validation or save error.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Record a profile path when the table is loaded from one.
    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.file.path = path;
    }

    /// Number of tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Insert or replace a validated tool.
    ///
    /// # Errors
    ///
    /// Returns the record's [`ValidationError`]; nothing is changed on failure.
    pub fn insert(&mut self, record: ToolRecord) -> Result<(), ValidationError> {
        record.validate()?;
        self.table.insert(record);
        self.dirty = true;
        self.error = None;
        self.file.status = "UNSAVED CHANGES".to_string();
        Ok(())
    }

    /// Remove one tool by number.
    ///
    /// Returns `true` when a record was removed.
    pub fn remove(&mut self, number: u32) -> bool {
        if self.table.remove(number).is_none() {
            return false;
        }
        self.dirty = true;
        self.error = None;
        self.file.status = "UNSAVED CHANGES".to_string();
        true
    }

    /// Build a validated record from text fields.
    ///
    /// Numeric text is parsed in millimetres; an empty pocket is `None`.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message for malformed text, an invalid
    /// dimension, or an over-long/control-character name.
    pub fn record_from_text(
        number: &str,
        name: &str,
        diameter: &str,
        length: &str,
        pocket: &str,
    ) -> Result<ToolRecord, String> {
        let number = parse_u32(number, "tool number")?;
        let diameter = parse_f64(diameter, "diameter")?;
        let length = parse_f64(length, "length")?;
        let pocket = if pocket.trim().is_empty() {
            None
        } else {
            Some(parse_u32(pocket, "pocket")?)
        };
        let record = ToolRecord {
            number,
            name: name.trim().to_string(),
            diameter_mm: diameter,
            length_mm: length,
            pocket,
        };
        record.validate().map_err(|error| error.to_string())?;
        Ok(record)
    }

    /// Mark the table saved to `path`.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.dirty = false;
        self.error = None;
        self.file.path = Some(path);
        self.file.status = format!("SAVED ({} TOOLS)", self.table.len());
    }

    /// Record a failed validation or save without changing the table.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.file.status = "ERROR".to_string();
    }

    /// Set the status line without marking an error.
    pub fn set_status(&mut self, status: impl Into<String>) {
        self.file.status = status.into();
    }

    /// Display summary used by the tools table and the M6 sheet.
    #[must_use]
    pub fn summary(record: &ToolRecord) -> String {
        let mut parts = Vec::new();
        if record.diameter_mm > 0.0 {
            parts.push(format!("D{:.3} mm", record.diameter_mm));
        } else {
            parts.push("D --".to_string());
        }
        if record.length_mm > 0.0 {
            parts.push(format!("L{:.3} mm", record.length_mm));
        } else {
            parts.push("L --".to_string());
        }
        if let Some(pocket) = record.pocket {
            parts.push(format!("POCKET {pocket}"));
        }
        parts.join(" | ")
    }

    /// Display name for the M6 sheet, falling back to `T<number>`.
    #[must_use]
    pub fn display_name(record: &ToolRecord) -> String {
        if record.name.trim().is_empty() {
            format!("T{}", record.number)
        } else {
            record.name.trim().to_string()
        }
    }

    /// Full profile path, when known.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.file.path.as_deref()
    }
}

fn parse_u32(text: &str, field: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("{field} is required"));
    }
    text.parse::<u32>()
        .map_err(|_| format!("{field} must be a whole number"))
}

fn parse_f64(text: &str, field: &str) -> Result<f64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(0.0);
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| format!("{field} must be a number"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{field} must be finite and non-negative"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(number: u32, name: &str, diameter: f64) -> ToolRecord {
        ToolRecord {
            number,
            name: name.to_string(),
            diameter_mm: diameter,
            length_mm: 30.0,
            pocket: Some(number),
        }
    }

    #[test]
    fn insert_sorts_replaces_and_marks_dirty() {
        let mut model = ToolTableModel::new();
        model.insert(record(7, "Seven", 7.0)).expect("insert");
        model.insert(record(3, "Three", 3.0)).expect("insert");
        assert_eq!(
            model
                .table()
                .iter()
                .map(|tool| tool.number)
                .collect::<Vec<_>>(),
            vec![3, 7]
        );
        assert!(model.dirty());

        model.insert(record(3, "Three v2", 3.5)).expect("replace");
        assert_eq!(model.len(), 2);
        assert_eq!(
            model.get(3).map(|tool| tool.name.as_str()),
            Some("Three v2")
        );
        assert_eq!(model.get(3).map(|tool| tool.diameter_mm), Some(3.5));

        assert!(model.remove(7));
        assert!(!model.remove(7));
        assert_eq!(model.len(), 1);
    }

    #[test]
    fn invalid_records_are_rejected_without_changing_the_table() {
        let mut model = ToolTableModel::new();
        let bad = ToolRecord {
            number: 4,
            name: "Bad".into(),
            diameter_mm: -1.0,
            length_mm: 10.0,
            pocket: None,
        };
        assert!(model.insert(bad).is_err());
        assert!(model.is_empty());
        assert!(!model.dirty());

        let long_name = "x".repeat(129);
        assert!(
            ToolTableModel::record_from_text("5", &long_name, "1", "2", "")
                .expect_err("name limit")
                .contains("invalid")
        );
        assert!(ToolTableModel::record_from_text("x", "T", "1", "2", "").is_err());
        assert!(ToolTableModel::record_from_text("1", "T", "-1", "2", "").is_err());
    }

    #[test]
    fn record_from_text_parses_optional_fields() {
        let record = ToolTableModel::record_from_text(" 2 ", " Flat 3mm ", "3.0", "30.5", " 4 ")
            .expect("valid");
        assert_eq!(record.number, 2);
        assert_eq!(record.name, "Flat 3mm");
        assert_eq!(record.diameter_mm, 3.0);
        assert_eq!(record.length_mm, 30.5);
        assert_eq!(record.pocket, Some(4));

        let no_pocket = ToolTableModel::record_from_text("2", "Flat", "3", "", "")
            .expect("valid without pocket");
        assert_eq!(no_pocket.pocket, None);
        assert_eq!(no_pocket.length_mm, 0.0);
    }

    #[test]
    fn summaries_are_stable_for_the_m6_sheet() {
        let record = record(3, "6 mm flat", 6.0);
        assert_eq!(
            ToolTableModel::summary(&record),
            "D6.000 mm | L30.000 mm | POCKET 3"
        );
        assert_eq!(ToolTableModel::display_name(&record), "6 mm flat");
        let unnamed = ToolRecord {
            number: 9,
            name: String::new(),
            diameter_mm: 0.0,
            length_mm: 0.0,
            pocket: None,
        };
        assert_eq!(ToolTableModel::display_name(&unnamed), "T9");
        assert_eq!(ToolTableModel::summary(&unnamed), "D -- | L --");
    }

    #[test]
    fn loading_a_profile_clears_dirty_state() {
        let mut table = ToolTable::new();
        table.insert(record(1, "One", 1.0));
        let mut model = ToolTableModel::new();
        model.adopt(
            table,
            Some(PathBuf::from("/tmp/profile.json")),
            "1 TOOLS, SAVED",
        );
        assert_eq!(model.len(), 1);
        assert!(!model.dirty());
        assert_eq!(model.file.status, "1 TOOLS, SAVED");
        assert_eq!(model.path(), Some(Path::new("/tmp/profile.json")));
    }
}
