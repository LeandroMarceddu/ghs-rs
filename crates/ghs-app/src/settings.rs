//! Generated settings-editor model.
//!
//! The form is derived from the controller's `$$` values plus `$ES`/`$EG`
//! metadata. Nothing here names a setting number: rows are keyed by the
//! numbers the controller reports, typed from `[SETTING:...]` records, and
//! grouped by `[SETTINGGROUP:...]` parents.
//!
//! Editing is explicit. A control only changes a local draft; [`begin_apply`]
//! queues the changed rows and the shell sends one `Command::SetSetting` at a
//! time. A row is not marked applied until `SettingWritten { verified: true }`
//! arrives. The model owns no GPUI types, so the form rules are testable
//! without a window or an engine thread.
//!
//! [`begin_apply`]: SettingsModel::begin_apply

use std::collections::{HashMap, HashSet, VecDeque};

use ghs_engine::{Setting, SettingDescription, SettingGroup, SettingValue};
use ghs_i18n::t;

/// App-side mirror of the `$ES` type code.
///
/// The protocol enum is not re-exported by `ghs-engine`, and the UI should not
/// depend on `ghs-proto` directly. The mapping is the documented wire contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingKind {
    /// Type 0: boolean checkbox.
    Boolean,
    /// Type 1: bitfield checkbox column.
    Bitfield,
    /// Type 2: exclusive bitfield (bit 0 enables the other bits).
    ExclusiveBitfield,
    /// Type 3: radio buttons.
    Radio,
    /// Type 4: axis-mask checkbox column.
    AxisMask,
    /// Type 5: integer field.
    Integer,
    /// Type 6: decimal field.
    Decimal,
    /// Type 7: string field.
    Text,
    /// Type 8: password field.
    Password,
    /// Type 9: IPv4 address field.
    IpAddress,
    /// An unknown type code, shown read-only for forward compatibility.
    Unknown(u8),
}

impl SettingKind {
    /// Map an `$ES` type code to a UI kind.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => SettingKind::Boolean,
            1 => SettingKind::Bitfield,
            2 => SettingKind::ExclusiveBitfield,
            3 => SettingKind::Radio,
            4 => SettingKind::AxisMask,
            5 => SettingKind::Integer,
            6 => SettingKind::Decimal,
            7 => SettingKind::Text,
            8 => SettingKind::Password,
            9 => SettingKind::IpAddress,
            other => SettingKind::Unknown(other),
        }
    }

    /// Infer a UI kind from a value when no `$ES` metadata was reported.
    #[must_use]
    pub fn infer(value: &SettingValue) -> Self {
        match value {
            SettingValue::Bool(_) => SettingKind::Boolean,
            SettingValue::Int(_) => SettingKind::Integer,
            SettingValue::Float(_) => SettingKind::Decimal,
            SettingValue::String(_) | SettingValue::AxisArray(_) => SettingKind::Text,
        }
    }

    /// Whether the value is edited as a bit mask.
    #[must_use]
    pub const fn is_mask(self) -> bool {
        matches!(
            self,
            SettingKind::Bitfield | SettingKind::ExclusiveBitfield | SettingKind::AxisMask
        )
    }

    /// Whether the value accepts whole numbers only.
    #[must_use]
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            SettingKind::Bitfield
                | SettingKind::ExclusiveBitfield
                | SettingKind::AxisMask
                | SettingKind::Integer
                | SettingKind::Radio
        )
    }

    /// Whether the value is free-form text.
    #[must_use]
    pub const fn is_text(self) -> bool {
        matches!(
            self,
            SettingKind::Text | SettingKind::Password | SettingKind::IpAddress
        )
    }

    /// Short label for a row's metadata line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SettingKind::Boolean => "boolean",
            SettingKind::Bitfield => "bitfield",
            SettingKind::ExclusiveBitfield => "exclusive bitfield",
            SettingKind::Radio => "radio",
            SettingKind::AxisMask => "axis mask",
            SettingKind::Integer => "integer",
            SettingKind::Decimal => "decimal",
            SettingKind::Text => "text",
            SettingKind::Password => "password",
            SettingKind::IpAddress => "IPv4",
            SettingKind::Unknown(_) => "unsupported",
        }
    }
}

/// Typed `$ES` metadata, independent of the protocol crate.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingMeta {
    /// Setting number.
    pub number: u16,
    /// Group id reported by `$ES`, when present.
    pub group: Option<u16>,
    /// Display name.
    pub name: String,
    /// Unit label; `N/A` is already normalised to `None` by the parser.
    pub unit: Option<String>,
    /// Control kind.
    pub kind: SettingKind,
    /// Format hint such as `#####0.000`.
    pub format: Option<String>,
    /// Minimum accepted value.
    pub min: Option<f64>,
    /// Maximum accepted value.
    pub max: Option<f64>,
    /// Whether a reboot is required after a change.
    pub reboot: bool,
    /// Whether the value may be left blank.
    pub null_allowed: bool,
    /// Optional label for the blank value.
    pub null_label: Option<String>,
}

impl SettingMeta {
    /// Convert one `[SETTING:...]` record into the app's metadata row.
    #[must_use]
    pub fn from_description(description: &SettingDescription) -> Self {
        Self {
            number: description.id,
            group: description.group,
            name: description.name.clone(),
            unit: description.unit.clone(),
            kind: SettingKind::from_code(description.setting_type.code()),
            format: description.format.clone(),
            min: description.min,
            max: description.max,
            reboot: description.reboot_required,
            null_allowed: description.null_allowed,
            null_label: description.null_label.clone(),
        }
    }
}

/// One `[SETTINGGROUP:...]` record, independent of the protocol crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupMeta {
    /// Group id.
    pub id: u16,
    /// Parent group id.
    pub parent: Option<u16>,
    /// Display name.
    pub name: String,
}

impl GroupMeta {
    /// Convert one `[SETTINGGROUP:...]` record.
    #[must_use]
    pub fn from_group(group: &SettingGroup) -> Self {
        Self {
            id: group.id,
            parent: group.parent,
            name: group.name.clone(),
        }
    }
}

/// The group rail selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsScope {
    /// Every setting, flat.
    #[default]
    All,
    /// Settings whose metadata has no group (the fallback group).
    General,
    /// One `$EG` group and its children.
    Group(u16),
}

/// The local edit buffer for one row.
#[derive(Clone, Debug, PartialEq)]
enum SettingDraft {
    Bool(bool),
    Number(String),
    Mask(u64),
    Text(String),
}

/// Write/validation state of one row.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum RowStatus {
    /// No local change.
    #[default]
    Synced,
    /// A local change is waiting for Apply.
    Edited,
    /// The draft is not valid and cannot be applied.
    Invalid,
    /// The shell queued the write.
    Queued,
    /// The engine accepted the command; waiting for read-back.
    Writing,
    /// The controller confirmed the value with a matching read-back.
    Applied,
    /// The write or read-back failed; the previous value is kept.
    Failed(String),
}

impl RowStatus {
    /// Whether the row is waiting for the operator or the controller.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            RowStatus::Edited | RowStatus::Invalid | RowStatus::Queued | RowStatus::Writing
        )
    }

    /// Short label for the row header.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            RowStatus::Synced => "",
            RowStatus::Edited => "PENDING",
            RowStatus::Invalid => "INVALID",
            RowStatus::Queued => "QUEUED",
            RowStatus::Writing => "WRITING",
            RowStatus::Applied => "APPLIED",
            RowStatus::Failed(_) => "FAILED",
        }
    }
}

/// One generated form row.
#[derive(Clone, Debug)]
pub struct SettingRow {
    /// Setting number, the `<n>` in `$<n>`.
    pub number: u16,
    /// Display name from `$ES`, or the setting description, or `$<n>`.
    pub name: String,
    /// Control kind.
    pub kind: SettingKind,
    /// Group id from `$ES`; `None` is the fallback General group.
    pub group_id: Option<u16>,
    /// Group name resolved from `$EG`.
    pub group_name: String,
    /// Unit label, absent when the controller reports `N/A`.
    pub unit: Option<String>,
    /// Format hint from `$ES`.
    pub format: Option<String>,
    /// Minimum accepted value.
    pub min: Option<f64>,
    /// Maximum accepted value.
    pub max: Option<f64>,
    /// Whether the setting needs a reboot.
    pub reboot: bool,
    /// Whether the value may be blank.
    pub null_allowed: bool,
    /// Optional blank-value label.
    pub null_label: Option<String>,
    /// Last controller value (the "previous value" kept after a failure).
    pub current: SettingValue,
    /// Local edit state.
    pub status: RowStatus,
    /// Detail for an invalid or failed row.
    pub note: Option<String>,
    /// Whether the row came from `$ES` instead of value-type inference.
    pub described: bool,
    draft: SettingDraft,
}

impl SettingRow {
    /// Boolean draft.
    #[must_use]
    pub fn is_on(&self) -> bool {
        matches!(self.draft, SettingDraft::Bool(true))
    }

    /// Bit-mask draft.
    #[must_use]
    pub fn bits(&self) -> u64 {
        match self.draft {
            SettingDraft::Mask(bits) => bits,
            _ => 0,
        }
    }

    /// Radio draft interpreted as an integer.
    #[must_use]
    pub fn radio_value(&self) -> Option<i64> {
        match &self.draft {
            SettingDraft::Number(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Number of bit checkboxes to show for a mask.
    #[must_use]
    pub fn bit_count(&self) -> usize {
        if !self.kind.is_mask() {
            return 0;
        }
        if self.kind == SettingKind::AxisMask {
            return 9;
        }
        let Some(max) = self.max else {
            return 8;
        };
        if max < 1.0 {
            return 1;
        }
        ((max.log2().floor() as usize) + 1).clamp(1, 16)
    }

    /// Numeric draft as text, for the keypad-seeded field.
    #[must_use]
    pub fn number_text(&self) -> &str {
        match &self.draft {
            SettingDraft::Number(text) => text,
            _ => "",
        }
    }

    /// Text draft, without password masking.
    #[must_use]
    pub fn text_draft(&self) -> &str {
        match &self.draft {
            SettingDraft::Text(text) => text,
            _ => "",
        }
    }

    /// Draft value shown in the row's value field.
    #[must_use]
    pub fn display_value(&self) -> String {
        match &self.draft {
            SettingDraft::Bool(value) => {
                if *value {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                }
            }
            SettingDraft::Number(text) => text.clone(),
            SettingDraft::Mask(bits) => bits.to_string(),
            SettingDraft::Text(text) => {
                if self.kind == SettingKind::Password {
                    if text.is_empty() {
                        String::new()
                    } else {
                        "*".repeat(text.chars().count().clamp(6, 12))
                    }
                } else {
                    text.clone()
                }
            }
        }
    }

    /// Number of decimal places declared by the format hint.
    #[must_use]
    pub fn decimals(&self) -> usize {
        let Some(format) = self.format.as_deref() else {
            return 0;
        };
        format
            .split_once('.')
            .map(|(_, decimals)| decimals.chars().take_while(char::is_ascii_digit).count())
            .unwrap_or(0)
            .min(6)
    }

    /// Range text such as `1.000 .. 10000.000`.
    #[must_use]
    pub fn range_text(&self) -> String {
        if self.kind.is_text() {
            return "range N/A".to_string();
        }
        if matches!(self.kind, SettingKind::Boolean) {
            return "0 = OFF, 1 = ON".to_string();
        }
        let min = self.min.map(|value| self.format_bound(value));
        let max = self.max.map(|value| self.format_bound(value));
        match (min, max) {
            (Some(min), Some(max)) => format!("{min} .. {max}"),
            (Some(min), None) => format!(">= {min}"),
            (None, Some(max)) => format!("<= {max}"),
            (None, None) => "range N/A".to_string(),
        }
    }

    /// Dim metadata line under the name.
    #[must_use]
    pub fn meta_line(&self) -> String {
        let mut parts = Vec::new();
        parts.push(self.kind.as_str().to_uppercase());
        if let Some(unit) = &self.unit {
            parts.push(unit.to_uppercase());
        }
        parts.push(self.range_text());
        if let Some(format) = &self.format {
            parts.push(format!("{} {format}", t("FORMAT")));
        }
        if self.reboot {
            parts.push(t("REBOOT REQUIRED").to_string());
        }
        if self.null_allowed {
            parts.push(self.null_label.as_ref().map_or_else(
                || t("BLANK ALLOWED").to_string(),
                |label| format!("BLANK = {label}"),
            ));
        }
        if self.kind == SettingKind::ExclusiveBitfield {
            parts.push(t("BIT 0 ENABLES THE OTHERS").to_string());
        }
        if !self.described {
            parts.push(t("INFERRED FROM VALUE").to_string());
        }
        parts.join(" | ")
    }

    /// Format one bound with the setting's decimal count.
    #[must_use]
    pub fn format_bound(&self, value: f64) -> String {
        let decimals = self.decimals();
        if decimals == 0 {
            format!("{value:.0}")
        } else {
            format!("{value:.decimals$}")
        }
    }

    /// The value that Apply would send for this row.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason when the draft is out of range or
    /// malformed.
    pub fn expected_value(&self) -> Result<SettingValue, String> {
        validate_row(self)
    }
}

/// One row in the group rail.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRow {
    /// Scope selected by this row.
    pub scope: SettingsScope,
    /// Display name.
    pub name: String,
    /// Nesting depth for indentation.
    pub depth: u8,
    /// Settings in this group and its children.
    pub count: usize,
    /// Pending local edits in this subtree.
    pub pending: usize,
}

/// Generated settings state.
#[derive(Debug, Default)]
pub struct SettingsModel {
    settings: Vec<Setting>,
    meta: Vec<SettingMeta>,
    groups: Vec<GroupMeta>,
    axis_labels: Vec<String>,
    rows: Vec<SettingRow>,
    rail: Vec<GroupRow>,
    scope: SettingsScope,
    search: String,
    scope_chosen: bool,
    queue: VecDeque<u16>,
    inflight: Option<(u16, SettingValue)>,
    last_result: Option<String>,
    last_result_ok: bool,
}

impl SettingsModel {
    /// Number of generated rows.
    #[must_use]
    pub fn total(&self) -> usize {
        self.rows.len()
    }

    /// Axis labels used by an axis-mask row.
    #[must_use]
    pub fn axis_labels(&self) -> &[String] {
        &self.axis_labels
    }

    /// Replace the reported axes and rebuild.
    pub fn set_axis_labels(&mut self, labels: Vec<String>) {
        if self.axis_labels == labels {
            return;
        }
        self.axis_labels = labels;
        self.rebuild();
    }

    /// Replace the controller's values and rebuild.
    pub fn apply_settings(&mut self, settings: &[Setting]) {
        self.settings = settings.to_vec();
        self.rebuild();
    }

    /// Replace typed metadata from `$ES` and rebuild.
    pub fn apply_descriptions(&mut self, descriptions: &[SettingDescription]) {
        self.meta = descriptions
            .iter()
            .map(SettingMeta::from_description)
            .collect();
        self.rebuild();
    }

    /// Replace groups from `$EG` and rebuild.
    pub fn apply_groups(&mut self, groups: &[SettingGroup]) {
        self.groups = groups.iter().map(GroupMeta::from_group).collect();
        self.rebuild();
    }

    /// Replace both metadata sets in one rebuild.
    pub fn apply_metadata(&mut self, descriptions: &[SettingDescription], groups: &[SettingGroup]) {
        self.meta = descriptions
            .iter()
            .map(SettingMeta::from_description)
            .collect();
        self.groups = groups.iter().map(GroupMeta::from_group).collect();
        self.rebuild();
    }

    /// Current group selection.
    #[must_use]
    pub fn scope(&self) -> SettingsScope {
        self.scope
    }

    /// Select a group; an unknown id falls back to All.
    pub fn set_scope(&mut self, scope: SettingsScope) {
        if self.rail.iter().any(|row| row.scope == scope) {
            self.scope = scope;
            self.scope_chosen = true;
        } else {
            self.scope = SettingsScope::All;
            self.scope_chosen = false;
        }
    }

    /// Current search query.
    #[must_use]
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Update the search query.
    pub fn set_search(&mut self, query: impl Into<String>) {
        self.search = query.into();
    }

    /// Group rail, including the leading All row.
    #[must_use]
    pub fn rail(&self) -> &[GroupRow] {
        &self.rail
    }

    /// Name of the active group, or `Search results`.
    #[must_use]
    pub fn scope_name(&self) -> String {
        if !self.search.trim().is_empty() {
            return "Search results".to_string();
        }
        self.rail
            .iter()
            .find(|row| row.scope == self.scope)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| "All settings".to_string())
    }

    /// Rows for the active group and search.
    #[must_use]
    pub fn visible_rows(&self) -> Vec<&SettingRow> {
        let query = normalise_query(&self.search);
        self.rows
            .iter()
            .filter(|row| {
                if query.is_empty() {
                    self.row_in_scope(row)
                } else {
                    row.number.to_string().contains(&query)
                        || row.name.to_lowercase().contains(&query)
                        || row.group_name.to_lowercase().contains(&query)
                }
            })
            .collect()
    }

    /// One row by setting number.
    #[must_use]
    pub fn row(&self, number: u16) -> Option<&SettingRow> {
        self.rows.iter().find(|row| row.number == number)
    }

    /// Every generated row, in setting-number order.
    #[must_use]
    pub fn rows(&self) -> &[SettingRow] {
        &self.rows
    }

    /// Number of rows waiting for Apply or a read-back.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.status.is_pending())
            .count()
    }

    /// Whether at least one row has a local edit.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending_count() > 0
    }

    /// Whether a queued write is waiting to be sent.
    #[must_use]
    pub fn has_queued_write(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Setting number with a write sent and awaiting read-back.
    #[must_use]
    pub fn inflight_number(&self) -> Option<u16> {
        self.inflight.as_ref().map(|(number, _)| *number)
    }

    /// Last write result for the settings footer.
    #[must_use]
    pub fn last_result(&self) -> Option<&str> {
        self.last_result.as_deref()
    }

    /// Whether the last write result was a success.
    #[must_use]
    pub fn last_result_ok(&self) -> bool {
        self.last_result_ok
    }

    /// Toggle a boolean draft.
    pub fn set_bool(&mut self, number: u16, value: bool) {
        if let Some(row) = self.row_mut(number) {
            row.draft = SettingDraft::Bool(value);
            refresh_status(row);
        }
    }

    /// Set one bit of a mask draft.
    pub fn toggle_mask_bit(&mut self, number: u16, bit: u8, value: bool) {
        if bit >= 64 {
            return;
        }
        if let Some(row) = self.row_mut(number) {
            let mask = 1_u64 << bit;
            let bits = match row.draft {
                SettingDraft::Mask(bits) => bits,
                _ => 0,
            };
            let mut next = if value { bits | mask } else { bits & !mask };
            // An exclusive bitfield's bit 0 is the master switch: turning it
            // off clears every subordinate bit instead of leaving a value that
            // the controller would ignore.
            if row.kind == SettingKind::ExclusiveBitfield && bit == 0 && !value {
                next = 0;
            }
            row.draft = SettingDraft::Mask(next);
            refresh_status(row);
        }
    }

    /// Set a radio draft.
    pub fn set_radio(&mut self, number: u16, value: i64) {
        if let Some(row) = self.row_mut(number) {
            row.draft = SettingDraft::Number(value.to_string());
            refresh_status(row);
        }
    }

    /// Replace a numeric draft with typed text.
    ///
    /// Invalid text is kept so the operator can see and correct it; the row is
    /// marked [`RowStatus::Invalid`] and Apply will refuse it.
    ///
    /// # Errors
    ///
    /// Returns the validation message for the console.
    pub fn set_number_text(&mut self, number: u16, text: impl Into<String>) -> Result<(), String> {
        let Some(row) = self.row_mut(number) else {
            return Err("setting is not in the current snapshot".to_string());
        };
        row.draft = SettingDraft::Number(text.into());
        refresh_status(row);
        match row.expected_value() {
            Ok(_) => Ok(()),
            Err(message) => Err(message),
        }
    }

    /// Replace a text draft.
    ///
    /// # Errors
    ///
    /// Returns the validation message (required value, malformed IPv4).
    pub fn set_text_draft(&mut self, number: u16, text: impl Into<String>) -> Result<(), String> {
        let Some(row) = self.row_mut(number) else {
            return Err("setting is not in the current snapshot".to_string());
        };
        row.draft = SettingDraft::Text(text.into());
        refresh_status(row);
        match row.expected_value() {
            Ok(_) => Ok(()),
            Err(message) => Err(message),
        }
    }

    /// Discard every local edit and cancel queued writes.
    pub fn reset_drafts(&mut self) {
        self.queue.clear();
        self.inflight = None;
        for row in &mut self.rows {
            row.draft = draft_from_value(&row.current, row.kind, row.format.as_deref());
            row.status = RowStatus::Synced;
            row.note = None;
        }
        self.last_result = None;
        self.last_result_ok = false;
    }

    /// Validate every changed row and queue the valid ones.
    ///
    /// # Errors
    ///
    /// Returns a message naming the invalid rows. Nothing is queued in that
    /// case, so a typo cannot partially apply a batch.
    pub fn begin_apply(&mut self) -> Result<usize, String> {
        if self.inflight.is_some() || !self.queue.is_empty() {
            return Err("a settings write is already in progress".to_string());
        }
        let mut edited = Vec::new();
        let mut invalid = Vec::new();
        for row in &self.rows {
            let changed = match validate_row(row) {
                Ok(value) => value.to_string() != row.current.to_string(),
                Err(_) => true,
            };
            if !changed {
                continue;
            }
            if validate_row(row).is_ok() {
                edited.push(row.number);
            } else {
                invalid.push(row.number);
            }
        }
        if !invalid.is_empty() {
            let list = invalid
                .iter()
                .map(|number| format!("${number}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("fix invalid value(s) before applying: {list}"));
        }
        for number in &edited {
            if let Some(row) = self.row_mut(*number) {
                row.status = RowStatus::Queued;
                row.note = None;
            }
        }
        self.queue = edited.iter().copied().collect();
        Ok(edited.len())
    }

    /// Pop the next write when no other write is in flight.
    ///
    /// The engine verifies one setting at a time, so the caller waits for
    /// [`SettingsModel::written`] before asking again.
    pub fn take_next_write(&mut self) -> Option<(u16, SettingValue)> {
        if self.inflight.is_some() {
            return None;
        }
        while let Some(number) = self.queue.pop_front() {
            let Some(row) = self.row_mut(number) else {
                continue;
            };
            match validate_row(row) {
                Ok(value) => {
                    row.status = RowStatus::Writing;
                    row.note = None;
                    self.inflight = Some((number, value.clone()));
                    return Some((number, value));
                }
                Err(message) => {
                    row.status = RowStatus::Invalid;
                    row.note = Some(message);
                }
            }
        }
        None
    }

    /// Put a write back at the front of the queue after a transient busy
    /// rejection, before anything reached the controller.
    pub fn requeue_write(&mut self, number: u16) {
        if self
            .inflight
            .as_ref()
            .is_some_and(|(pending, _)| *pending == number)
        {
            self.inflight = None;
        }
        if let Some(row) = self.row_mut(number)
            && row.status == RowStatus::Writing
        {
            row.status = RowStatus::Queued;
        }
        if !self.queue.contains(&number) {
            self.queue.push_front(number);
        }
    }

    /// Apply `SettingWritten` from the engine.
    pub fn written(&mut self, number: u16, value: &SettingValue, verified: bool) {
        if self
            .inflight
            .as_ref()
            .is_some_and(|(pending, _)| *pending == number)
        {
            self.inflight = None;
        }
        self.queue.retain(|queued| *queued != number);
        let Some(row) = self.row_mut(number) else {
            self.last_result = Some(format!("${number} is not in the current snapshot"));
            self.last_result_ok = false;
            return;
        };
        if verified {
            row.current = value.clone();
            row.draft = draft_from_value(value, row.kind, row.format.as_deref());
            row.status = RowStatus::Applied;
            row.note = None;
            self.last_result = Some(format!("${number} APPLIED, READ-BACK VERIFIED ({value})"));
            self.last_result_ok = true;
        } else {
            let previous = row.current.clone();
            row.draft = draft_from_value(&previous, row.kind, row.format.as_deref());
            row.status = RowStatus::Failed(format!("read back {value}"));
            row.note = Some("Read-back did not match; the previous value is kept".to_string());
            self.last_result = Some(format!("${number} read back as {value}; not applied"));
            self.last_result_ok = false;
        }
    }

    /// Record an engine rejection before the command reached the controller.
    pub fn mark_send_error(&mut self, number: u16, message: impl Into<String>) {
        let message = message.into();
        if self
            .inflight
            .as_ref()
            .is_some_and(|(pending, _)| *pending == number)
        {
            self.inflight = None;
        }
        self.queue.retain(|queued| *queued != number);
        if let Some(row) = self.row_mut(number) {
            let previous = row.current.clone();
            row.draft = draft_from_value(&previous, row.kind, row.format.as_deref());
            row.status = RowStatus::Failed(message.clone());
            row.note = Some(message.clone());
        }
        self.last_result = Some(format!("${number} rejected: {message}"));
        self.last_result_ok = false;
    }

    /// Stop queued writes and return rows to the pending state.
    ///
    /// Used when a job starts, the link closes, or the controller leaves Idle.
    /// Returns `true` when there was something to cancel.
    pub fn cancel_writes(&mut self, reason: &str) -> bool {
        let had_work = !self.queue.is_empty() || self.inflight.is_some();
        self.queue.clear();
        self.inflight = None;
        if had_work {
            for row in &mut self.rows {
                if matches!(row.status, RowStatus::Queued | RowStatus::Writing) {
                    row.status = RowStatus::Edited;
                    row.note = Some(reason.to_string());
                }
            }
            self.last_result = Some(format!("Settings apply stopped: {reason}"));
            self.last_result_ok = false;
        }
        had_work
    }

    /// Whether the row is included in the active group.
    fn row_in_scope(&self, row: &SettingRow) -> bool {
        match self.scope {
            SettingsScope::All => true,
            SettingsScope::General => row.group_id.is_none(),
            SettingsScope::Group(id) => self.group_contains(row.group_id, id),
        }
    }

    /// Whether `group` is `target` or one of its descendants.
    fn group_contains(&self, group: Option<u16>, target: u16) -> bool {
        let mut current = group;
        let mut guard = 0;
        while let Some(id) = current {
            if id == target {
                return true;
            }
            current = self
                .groups
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent);
            guard += 1;
            if guard > 8 {
                return false;
            }
        }
        false
    }

    fn row_mut(&mut self, number: u16) -> Option<&mut SettingRow> {
        self.rows.iter_mut().find(|row| row.number == number)
    }

    /// Rebuild rows, groups, and selection from the stored controller data.
    fn rebuild(&mut self) {
        // Pending and result states survive a value refresh. A `Settings`
        // event follows every verified write, so dropping a draft here would
        // erase a verified read-back before the next frame.
        let preserved: HashMap<u16, (SettingDraft, RowStatus, Option<String>)> = self
            .rows
            .iter()
            .filter(|row| row.status != RowStatus::Synced)
            .map(|row| {
                (
                    row.number,
                    (row.draft.clone(), row.status.clone(), row.note.clone()),
                )
            })
            .collect();

        let meta_by_number: HashMap<u16, &SettingMeta> =
            self.meta.iter().map(|meta| (meta.number, meta)).collect();
        let group_by_id: HashMap<u16, &GroupMeta> =
            self.groups.iter().map(|group| (group.id, group)).collect();

        let mut rows = Vec::with_capacity(self.settings.len());
        for setting in &self.settings {
            let meta = meta_by_number.get(&setting.number).copied();
            let kind = meta.map_or_else(|| SettingKind::infer(&setting.value), |meta| meta.kind);
            let name = meta
                .map(|meta| meta.name.clone())
                .or_else(|| setting.description.clone())
                .unwrap_or_else(|| format!("${}", setting.number));
            let group_id = meta
                .and_then(|meta| meta.group)
                .filter(|id| group_by_id.contains_key(id))
                .or_else(|| {
                    setting.group.as_ref().and_then(|name| {
                        self.groups
                            .iter()
                            .find(|group| group.name.eq_ignore_ascii_case(name))
                            .map(|group| group.id)
                    })
                });
            let group_name = group_id
                .and_then(|id| group_by_id.get(&id))
                .map_or_else(|| "General".to_string(), |group| group.name.clone());
            let unit = meta
                .and_then(|meta| meta.unit.clone())
                .or_else(|| setting.unit.clone());
            let format = meta.and_then(|meta| meta.format.clone());
            let fresh_draft = draft_from_value(&setting.value, kind, format.as_deref());
            // An APPLIED row keeps its success marker only while the controller
            // still reports the value we wrote. An external change drops the
            // marker and shows the new value.
            let preserved_row = preserved.get(&setting.number).filter(|(draft, status, _)| {
                *status != RowStatus::Applied || draft_text(draft) == draft_text(&fresh_draft)
            });
            let (draft, status, note) =
                preserved_row
                    .cloned()
                    .unwrap_or((fresh_draft, RowStatus::Synced, None));
            rows.push(SettingRow {
                number: setting.number,
                name,
                kind,
                group_id,
                group_name,
                unit,
                format,
                min: meta.and_then(|meta| meta.min),
                max: meta.and_then(|meta| meta.max),
                reboot: meta.is_some_and(|meta| meta.reboot),
                null_allowed: meta.is_some_and(|meta| meta.null_allowed),
                null_label: meta.and_then(|meta| meta.null_label.clone()),
                current: setting.value.clone(),
                status,
                note,
                described: meta.is_some(),
                draft,
            });
        }
        rows.sort_by_key(|row| row.number);

        let ungrouped = rows.iter().filter(|row| row.group_id.is_none()).count();
        let ungrouped_pending = rows
            .iter()
            .filter(|row| row.group_id.is_none())
            .filter(|row| row.status.is_pending())
            .count();
        self.rows = rows;
        self.rail = self.build_rail(ungrouped, ungrouped_pending);

        if self.rows.is_empty() {
            self.scope = SettingsScope::All;
            self.scope_chosen = false;
        } else if !self.scope_chosen {
            // Values usually arrive before `$ES`/`$EG`; re-pick the preferred
            // group on every rebuild until the operator chooses one.
            self.scope = self.preferred_scope();
        } else if !self.rail.iter().any(|row| row.scope == self.scope) {
            self.scope = SettingsScope::All;
            self.scope_chosen = false;
        }

        if let Some((number, _)) = &self.inflight
            && !self.rows.iter().any(|row| row.number == *number)
        {
            self.inflight = None;
        }
    }

    fn build_rail(&self, ungrouped: usize, ungrouped_pending: usize) -> Vec<GroupRow> {
        let total_pending = self
            .rows
            .iter()
            .filter(|row| row.status.is_pending())
            .count();
        let mut rail = vec![GroupRow {
            scope: SettingsScope::All,
            name: "All settings".to_string(),
            depth: 0,
            count: self.rows.len(),
            pending: total_pending,
        }];
        if ungrouped > 0 {
            rail.push(GroupRow {
                scope: SettingsScope::General,
                name: "General".to_string(),
                depth: 0,
                count: ungrouped,
                pending: ungrouped_pending,
            });
        }

        let group_by_id: HashMap<u16, &GroupMeta> =
            self.groups.iter().map(|group| (group.id, group)).collect();
        let mut counts: HashMap<u16, (usize, usize)> = HashMap::new();
        for row in &self.rows {
            let mut current = row.group_id;
            let mut guard = 0;
            while let Some(id) = current {
                let Some(group) = group_by_id.get(&id) else {
                    break;
                };
                let entry = counts.entry(id).or_insert((0, 0));
                entry.0 += 1;
                if row.status.is_pending() {
                    entry.1 += 1;
                }
                current = group.parent;
                guard += 1;
                if guard > 8 {
                    break;
                }
            }
        }

        let mut children: HashMap<Option<u16>, Vec<u16>> = HashMap::new();
        for group in &self.groups {
            let parent = group
                .parent
                .filter(|parent| *parent != group.id && group_by_id.contains_key(parent));
            children.entry(parent).or_default().push(group.id);
        }
        for list in children.values_mut() {
            list.sort_unstable();
        }

        let mut emitted = HashSet::new();
        if let Some(roots) = children.get(&None).cloned() {
            for id in roots {
                self.push_group_tree(id, 0, &children, &counts, &mut rail, &mut emitted);
            }
        }
        rail
    }

    fn push_group_tree(
        &self,
        id: u16,
        depth: u8,
        children: &HashMap<Option<u16>, Vec<u16>>,
        counts: &HashMap<u16, (usize, usize)>,
        out: &mut Vec<GroupRow>,
        emitted: &mut HashSet<u16>,
    ) {
        if depth > 8 || !emitted.insert(id) {
            return;
        }
        let Some(group) = self.groups.iter().find(|group| group.id == id) else {
            return;
        };
        let (count, pending) = counts.get(&id).copied().unwrap_or((0, 0));
        if count > 0 {
            out.push(GroupRow {
                scope: SettingsScope::Group(id),
                name: group.name.clone(),
                depth,
                count,
                pending,
            });
        }
        if let Some(child_ids) = children.get(&Some(id)) {
            for child in child_ids {
                self.push_group_tree(*child, depth + 1, children, counts, out, emitted);
            }
        }
    }

    fn preferred_scope(&self) -> SettingsScope {
        self.rail
            .iter()
            .skip(1)
            .find(|row| row.depth > 0)
            .or_else(|| self.rail.get(1))
            .map_or(SettingsScope::All, |row| row.scope)
    }

    #[cfg(test)]
    fn apply_test_metadata(&mut self, meta: Vec<SettingMeta>, groups: Vec<GroupMeta>) {
        self.meta = meta;
        self.groups = groups;
        self.rebuild();
    }
}

/// Normalise a search query: lowercase, no `$` prefix.
fn normalise_query(query: &str) -> String {
    query.trim().trim_start_matches('$').to_lowercase()
}

/// Comparable text for a draft, used to detect an external value change.
fn draft_text(draft: &SettingDraft) -> String {
    match draft {
        SettingDraft::Bool(value) => {
            if *value {
                "ON".to_string()
            } else {
                "OFF".to_string()
            }
        }
        SettingDraft::Number(text) | SettingDraft::Text(text) => text.clone(),
        SettingDraft::Mask(bits) => bits.to_string(),
    }
}

/// Build the draft matching a controller value.
fn draft_from_value(value: &SettingValue, kind: SettingKind, format: Option<&str>) -> SettingDraft {
    match kind {
        SettingKind::Boolean => SettingDraft::Bool(value.as_bool().unwrap_or(false)),
        SettingKind::Bitfield | SettingKind::ExclusiveBitfield | SettingKind::AxisMask => {
            SettingDraft::Mask(value.as_f64().unwrap_or(0.0).max(0.0).min(u64::MAX as f64) as u64)
        }
        SettingKind::Integer | SettingKind::Decimal | SettingKind::Radio => {
            SettingDraft::Number(format_draft_number(value, format))
        }
        SettingKind::Text
        | SettingKind::Password
        | SettingKind::IpAddress
        | SettingKind::Unknown(_) => SettingDraft::Text(value.to_string()),
    }
}

/// Format a numeric value using the `$ES` decimal count.
fn format_draft_number(value: &SettingValue, format: Option<&str>) -> String {
    let decimals = format
        .and_then(|format| format.split_once('.'))
        .map(|(_, decimals)| decimals.chars().take_while(char::is_ascii_digit).count())
        .unwrap_or(0)
        .min(6);
    let Some(number) = value.as_f64() else {
        return value.to_string();
    };
    if decimals == 0 {
        format!("{number:.0}")
    } else {
        format!("{number:.decimals$}")
    }
}

/// Validate a row's draft and return the value to write.
fn validate_row(row: &SettingRow) -> Result<SettingValue, String> {
    match &row.draft {
        SettingDraft::Bool(value) => Ok(SettingValue::Bool(*value)),
        SettingDraft::Mask(bits) => {
            let value = *bits as f64;
            check_range(row, value)?;
            Ok(SettingValue::Int(*bits as i64))
        }
        SettingDraft::Number(text) => {
            let text = text.trim();
            if text.is_empty() {
                if row.null_allowed {
                    return Ok(SettingValue::String(String::new()));
                }
                return Err("a value is required".to_string());
            }
            if row.kind.is_integer() {
                let value = text
                    .parse::<i64>()
                    .map_err(|_| format!("'{text}' is not a whole number"))?;
                check_range(row, value as f64)?;
                Ok(SettingValue::Int(value))
            } else {
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("'{text}' is not a number"))?;
                if !value.is_finite() {
                    return Err("the value must be finite".to_string());
                }
                check_range(row, value)?;
                Ok(SettingValue::Float(value))
            }
        }
        SettingDraft::Text(text) => {
            let text = text.trim();
            if text.is_empty() && !row.null_allowed {
                return Err("a value is required".to_string());
            }
            if row.kind == SettingKind::IpAddress && !text.is_empty() && !is_ipv4(text) {
                return Err("expected an IPv4 address such as 192.168.1.10".to_string());
            }
            // Read-back compares the wire form. Parsing the text now mirrors
            // what the controller will echo, including numeric-looking text.
            Ok(SettingValue::parse(text))
        }
    }
}

/// Check a numeric value against the reported range.
fn check_range(row: &SettingRow, value: f64) -> Result<(), String> {
    if let Some(min) = row.min
        && value < min
    {
        let min = row.format_bound(min);
        return Err(format!("{value} is below the minimum {min}"));
    }
    if let Some(max) = row.max
        && value > max
    {
        let max = row.format_bound(max);
        return Err(format!("{value} is above the maximum {max}"));
    }
    Ok(())
}

/// Whether `text` is a dotted-quad IPv4 address.
fn is_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|ch| ch.is_ascii_digit())
                && part.parse::<u8>().is_ok()
        })
}

/// Recompute a row's status after a local edit.
fn refresh_status(row: &mut SettingRow) {
    match validate_row(row) {
        Ok(value) => {
            row.note = None;
            row.status = if value.to_string() == row.current.to_string() {
                RowStatus::Synced
            } else {
                RowStatus::Edited
            };
        }
        Err(message) => {
            row.status = RowStatus::Invalid;
            row.note = Some(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(number: u16, value: SettingValue) -> Setting {
        Setting::new(number, value)
    }

    fn group(id: u16, parent: Option<u16>, name: &str) -> GroupMeta {
        GroupMeta {
            id,
            parent,
            name: name.to_string(),
        }
    }

    fn meta(number: u16, group: Option<u16>, name: &str, kind: SettingKind) -> SettingMeta {
        SettingMeta {
            number,
            group,
            name: name.to_string(),
            unit: None,
            kind,
            format: None,
            min: None,
            max: None,
            reboot: false,
            null_allowed: false,
            null_label: None,
        }
    }

    fn model_with(
        settings: Vec<Setting>,
        meta: Vec<SettingMeta>,
        groups: Vec<GroupMeta>,
    ) -> SettingsModel {
        let mut model = SettingsModel::default();
        model.apply_settings(&settings);
        model.apply_test_metadata(meta, groups);
        model
    }

    #[test]
    fn groups_flatten_parents_and_filter_the_form() {
        let settings = vec![
            setting(0, SettingValue::Int(10)),
            setting(1, SettingValue::Int(25)),
            setting(10, SettingValue::Int(3)),
            setting(20, SettingValue::Int(0)),
            setting(30, SettingValue::Float(24_000.0)),
        ];
        let meta = vec![
            meta(0, Some(1), "Step pulse time", SettingKind::Integer),
            meta(1, Some(1), "Step idle delay", SettingKind::Integer),
            meta(10, Some(0), "Status report mask", SettingKind::Bitfield),
            meta(20, Some(2), "Soft limits enable", SettingKind::Boolean),
            meta(30, Some(3), "Maximum spindle speed", SettingKind::Decimal),
        ];
        let groups = vec![
            group(0, None, "General"),
            group(1, Some(0), "Stepper"),
            group(2, Some(0), "Limits"),
            group(3, Some(0), "Spindle"),
        ];
        let mut model = model_with(settings, meta, groups);

        let rail: Vec<(String, u8, usize)> = model
            .rail()
            .iter()
            .map(|row| (row.name.clone(), row.depth, row.count))
            .collect();
        assert_eq!(rail[0], ("All settings".to_string(), 0, 5));
        assert_eq!(rail[1], ("General".to_string(), 0, 5));
        assert_eq!(rail[2], ("Stepper".to_string(), 1, 2));
        assert_eq!(rail[3], ("Limits".to_string(), 1, 1));
        assert_eq!(rail[4], ("Spindle".to_string(), 1, 1));

        // The first leaf group is opened by default.
        assert_eq!(model.scope(), SettingsScope::Group(1));
        let visible: Vec<u16> = model.visible_rows().iter().map(|row| row.number).collect();
        assert_eq!(visible, vec![0, 1]);

        // Selecting the parent includes its children.
        model.set_scope(SettingsScope::Group(0));
        assert_eq!(model.visible_rows().len(), 5);

        // A group the controller did not report falls back to All.
        model.set_scope(SettingsScope::Group(99));
        assert_eq!(model.scope(), SettingsScope::All);
    }

    #[test]
    fn type_mapping_covers_metadata_and_value_fallbacks() {
        let codes = [
            (0, SettingKind::Boolean),
            (1, SettingKind::Bitfield),
            (2, SettingKind::ExclusiveBitfield),
            (3, SettingKind::Radio),
            (4, SettingKind::AxisMask),
            (5, SettingKind::Integer),
            (6, SettingKind::Decimal),
            (7, SettingKind::Text),
            (8, SettingKind::Password),
            (9, SettingKind::IpAddress),
            (42, SettingKind::Unknown(42)),
        ];
        for (code, kind) in codes {
            assert_eq!(SettingKind::from_code(code), kind);
        }

        // No `$ES`: type and group come from the value alone.
        let settings = vec![
            setting(4, SettingValue::Bool(true)),
            setting(5, SettingValue::Int(-3)),
            setting(6, SettingValue::Float(1.5)),
            setting(7, SettingValue::String("LAN".into())),
            setting(100, SettingValue::parse("250,250,250")),
        ];
        let model = model_with(settings, Vec::new(), Vec::new());
        let rows = model.visible_rows();
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|row| row.group_name == "General"));
        assert!(rows.iter().all(|row| !row.described));
        assert_eq!(rows[0].kind, SettingKind::Boolean);
        assert_eq!(rows[1].kind, SettingKind::Integer);
        assert_eq!(rows[2].kind, SettingKind::Decimal);
        assert_eq!(rows[3].kind, SettingKind::Text);
        // An axis array has no generated per-axis metadata; it stays editable
        // as the comma-separated text the controller reported.
        assert_eq!(rows[4].kind, SettingKind::Text);
        assert_eq!(rows[4].display_value(), "250,250,250");
        assert_eq!(rows[4].group_name, "General");
    }

    #[test]
    fn range_validation_rejects_bad_numbers_and_keeps_the_draft() {
        let mut integer = meta(1, None, "Step idle delay", SettingKind::Integer);
        integer.min = Some(0.0);
        integer.max = Some(255.0);
        let mut decimal = meta(100, None, "X steps per millimetre", SettingKind::Decimal);
        decimal.min = Some(1.0);
        decimal.max = Some(10_000.0);
        decimal.format = Some("#####0.000".to_string());
        let mut model = model_with(
            vec![
                setting(1, SettingValue::Int(25)),
                setting(100, SettingValue::Float(250.0)),
            ],
            vec![integer, decimal],
            Vec::new(),
        );

        let error = model.set_number_text(1, "300").expect_err("out of range");
        assert!(error.contains("maximum"), "{error}");
        assert_eq!(model.row(1).unwrap().status, RowStatus::Invalid);
        assert_eq!(model.pending_count(), 1);
        // One invalid row blocks the whole batch; nothing is queued.
        assert!(model.begin_apply().is_err());
        assert!(!model.has_queued_write());

        assert!(model.set_number_text(1, "12.5").is_err());
        assert!(model.set_number_text(1, "26").is_ok());
        assert_eq!(model.row(1).unwrap().status, RowStatus::Edited);
        assert_eq!(
            model.row(1).unwrap().expected_value().unwrap(),
            SettingValue::Int(26)
        );

        assert!(model.set_number_text(100, "1.5").is_ok());
        assert_eq!(
            model.row(100).unwrap().expected_value().unwrap(),
            SettingValue::Float(1.5)
        );
        assert!(model.set_number_text(100, "0.5").is_err());
    }

    #[test]
    fn writes_stay_pending_until_a_verified_read_back() {
        let mut model = model_with(
            vec![setting(22, SettingValue::Bool(false))],
            vec![meta(22, None, "Homing cycle enable", SettingKind::Boolean)],
            Vec::new(),
        );

        model.set_bool(22, true);
        assert_eq!(model.row(22).unwrap().status, RowStatus::Edited);
        assert_eq!(model.pending_count(), 1);

        assert_eq!(model.begin_apply().expect("valid batch"), 1);
        assert_eq!(model.row(22).unwrap().status, RowStatus::Queued);
        let (number, value) = model.take_next_write().expect("first write");
        assert_eq!(number, 22);
        assert_eq!(value, SettingValue::Bool(true));
        assert_eq!(model.inflight_number(), Some(22));
        assert_eq!(model.row(22).unwrap().status, RowStatus::Writing);
        assert!(model.take_next_write().is_none(), "one write at a time");

        // A failed read-back never marks the row applied and keeps the old
        // controller value visible.
        model.written(22, &SettingValue::Int(0), false);
        let row = model.row(22).unwrap();
        assert!(matches!(row.status, RowStatus::Failed(_)));
        assert_eq!(row.current, SettingValue::Bool(false));
        assert!(!row.is_on());
        assert_eq!(model.pending_count(), 0);
        assert!(!model.last_result_ok());

        // Retrying and getting a matching read-back applies it.
        model.set_bool(22, true);
        assert_eq!(model.begin_apply().expect("retry"), 1);
        let (number, _) = model.take_next_write().expect("retry write");
        assert_eq!(number, 22);
        model.written(22, &SettingValue::Bool(true), true);
        let row = model.row(22).unwrap();
        assert_eq!(row.status, RowStatus::Applied);
        assert_eq!(row.current, SettingValue::Bool(true));
        assert!(row.is_on());
        assert!(model.last_result_ok());

        // A later snapshot with a different value clears the APPLIED marker.
        model.apply_settings(&[setting(22, SettingValue::Bool(false))]);
        let row = model.row(22).unwrap();
        assert_eq!(row.status, RowStatus::Synced);
        assert!(!row.is_on());
    }

    #[test]
    fn exclusive_bitfield_clears_children_when_the_master_turns_off() {
        let mut master = meta(32, None, "Laser mode", SettingKind::ExclusiveBitfield);
        master.max = Some(255.0);
        let mut model = model_with(
            vec![setting(32, SettingValue::Int(0))],
            vec![master],
            Vec::new(),
        );

        model.toggle_mask_bit(32, 2, true);
        assert_eq!(model.row(32).unwrap().bits(), 0b100);
        model.toggle_mask_bit(32, 0, true);
        assert_eq!(model.row(32).unwrap().bits(), 0b101);
        model.toggle_mask_bit(32, 0, false);
        assert_eq!(model.row(32).unwrap().bits(), 0);
    }

    #[test]
    fn search_matches_number_name_and_group() {
        let settings = vec![
            setting(20, SettingValue::Int(0)),
            setting(100, SettingValue::Float(250.0)),
        ];
        let meta = vec![
            meta(20, Some(2), "Soft limits enable", SettingKind::Boolean),
            meta(100, Some(4), "X steps per millimetre", SettingKind::Decimal),
        ];
        let groups = vec![group(2, None, "Limits"), group(4, None, "Axes")];
        let mut model = model_with(settings, meta, groups);

        model.set_search("$100");
        let rows: Vec<u16> = model.visible_rows().iter().map(|row| row.number).collect();
        assert_eq!(rows, vec![100]);

        model.set_search("limits");
        let rows: Vec<u16> = model.visible_rows().iter().map(|row| row.number).collect();
        assert_eq!(rows, vec![20]);

        model.set_search("millimetre");
        let rows: Vec<u16> = model.visible_rows().iter().map(|row| row.number).collect();
        assert_eq!(rows, vec![100]);

        model.set_search("");
        model.set_scope(SettingsScope::All);
        assert_eq!(model.visible_rows().len(), 2);
    }

    #[test]
    fn busy_write_returns_to_the_queue() {
        let mut model = model_with(
            vec![setting(22, SettingValue::Bool(false))],
            vec![meta(22, None, "Homing cycle enable", SettingKind::Boolean)],
            Vec::new(),
        );
        model.set_bool(22, true);
        assert_eq!(model.begin_apply().expect("queue"), 1);
        let (number, _) = model.take_next_write().expect("first attempt");
        assert_eq!(number, 22);
        assert_eq!(model.inflight_number(), Some(22));

        model.requeue_write(22);
        assert_eq!(model.inflight_number(), None);
        assert_eq!(model.row(22).unwrap().status, RowStatus::Queued);
        assert_eq!(model.take_next_write().expect("retry").0, 22);
    }

    #[test]
    fn radio_and_ipv4_drafts_validate() {
        let mut radio = meta(3, None, "Work mode", SettingKind::Radio);
        radio.min = Some(0.0);
        radio.max = Some(3.0);
        let mut address = meta(40, None, "Network address", SettingKind::IpAddress);
        address.null_allowed = true;
        let mut model = model_with(
            vec![
                setting(3, SettingValue::Int(0)),
                setting(40, SettingValue::String("10.0.0.2".into())),
            ],
            vec![radio, address],
            Vec::new(),
        );

        model.set_radio(3, 2);
        assert_eq!(
            model.row(3).unwrap().expected_value().unwrap(),
            SettingValue::Int(2)
        );
        assert_eq!(model.row(3).unwrap().status, RowStatus::Edited);

        assert!(model.set_text_draft(40, "not-an-address").is_err());
        assert_eq!(model.row(40).unwrap().status, RowStatus::Invalid);
        assert!(model.set_text_draft(40, "192.168.1.10").is_ok());
        assert_eq!(
            model.row(40).unwrap().expected_value().unwrap(),
            SettingValue::String("192.168.1.10".into())
        );
        // `null_allowed` lets the operator clear an address.
        assert!(model.set_text_draft(40, "").is_ok());
        assert_eq!(
            model.row(40).unwrap().expected_value().unwrap(),
            SettingValue::String(String::new())
        );
    }
}
