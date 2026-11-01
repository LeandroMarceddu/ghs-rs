//! Live `$#` offset view model.
//!
//! The engine remains the only writer. This module maps one `ghs_engine::Offsets`
//! snapshot into display rows and formats the `G10 L20` lines the shell sends
//! through MDI. It owns no GPUI types and never talks to a transport, so the
//! mapping and command formatting are testable without a window or an engine.
//!
//! Work-offset edits are always `G10 L20 P<n> <axis><value>` in the selected
//! coordinate system. `G10` changes the stored offset; it never moves an axis.
//! `G92` is displayed but not edited here because it is non-persistent and is
//! cleared by a reset.

use ghs_config::OffsetSnapshot;
use ghs_engine::{Axes, Axis as CoreAxis, Offsets, Position, ProbeResult, Wcs};

use crate::state::{Axis, Units};

/// Largest accepted absolute offset in millimetres.
///
/// Every workspace in the reference envelope is far smaller. The bound keeps a
/// typo from producing an enormous coordinate and keeps the formatted line
/// well inside the controller receive buffer.
pub const MAX_OFFSET_MM: f64 = 100_000.0;

/// One work-coordinate row with values in display units.
#[derive(Clone, Debug, PartialEq)]
pub struct WcsRow {
    /// Coordinate system.
    pub wcs: Wcs,
    /// Whether this is the controller's active WCS.
    pub active: bool,
    /// Whether the controller reported this coordinate system.
    pub reported: bool,
    /// X, Y and Z values in display units; `None` when not reported.
    pub values: [Option<f64>; 3],
}

/// One read-only `$#` parameter row.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedRow {
    /// Parameter name, for example `G28` or `TLR`.
    pub label: &'static str,
    /// Values already formatted in display units.
    pub value: String,
    /// Optional caveat shown next to the value.
    pub note: Option<&'static str>,
}

/// Result of the last offset command, for the tab footer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OffsetStatus {
    /// No command has been sent in this session.
    Idle,
    /// The line was accepted and `$#` is refreshing.
    Refreshing,
    /// The last command was sent and the controller acknowledged the refresh.
    Refreshed,
    /// The engine or controller rejected the command.
    Failed(String),
}

impl OffsetStatus {
    /// Whether a read-only refresh is still expected.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        matches!(self, OffsetStatus::Refreshing)
    }
}

/// Mapping and formatting state for the offsets workspace.
#[derive(Clone, Debug)]
pub struct OffsetModel {
    offsets: Offsets,
    units: Units,
    axes: Vec<Axis>,
    active_wcs: usize,
    reported: bool,
    status: OffsetStatus,
    last_line: Option<String>,
}

impl Default for OffsetModel {
    fn default() -> Self {
        Self {
            offsets: Offsets::default(),
            units: Units::Millimetre,
            axes: (0..3).map(Axis::from_index).collect(),
            active_wcs: 0,
            reported: false,
            status: OffsetStatus::Idle,
            last_line: None,
        }
    }
}

impl OffsetModel {
    /// Replace the display context without touching the reported values.
    pub fn set_context(&mut self, units: Units, axes: &[Axis], active_wcs: usize) {
        self.units = units;
        self.axes.clear();
        self.axes.extend_from_slice(axes);
        self.active_wcs = active_wcs.min(Wcs::ALL.len().saturating_sub(1));
    }

    /// Install one engine `$#` snapshot.
    pub fn apply_engine(&mut self, offsets: &Offsets) {
        self.offsets = offsets.clone();
        self.reported = true;
        if self.status.is_pending() {
            self.status = OffsetStatus::Refreshed;
        }
    }

    /// Whether the controller has answered `$#` at least once.
    #[must_use]
    pub fn reported(&self) -> bool {
        self.reported
    }

    /// Current status.
    #[must_use]
    pub fn status(&self) -> &OffsetStatus {
        &self.status
    }

    /// Last line sent from this tab, if any.
    #[must_use]
    pub fn last_line(&self) -> Option<&str> {
        self.last_line.as_deref()
    }

    /// Record an accepted command and start waiting for the `$#` refresh.
    pub fn command_sent(&mut self, line: impl Into<String>) {
        self.last_line = Some(line.into());
        self.status = OffsetStatus::Refreshing;
    }

    /// Record a refresh request that did not carry a `G10` line.
    pub fn refresh_requested(&mut self) {
        self.status = OffsetStatus::Refreshing;
    }

    /// Record an engine or controller rejection.
    pub fn failed(&mut self, message: impl Into<String>) {
        self.status = OffsetStatus::Failed(message.into());
    }

    /// Nine work-coordinate rows in `$#` order.
    #[must_use]
    pub fn wcs_rows(&self) -> Vec<WcsRow> {
        Wcs::ALL
            .iter()
            .map(|wcs| {
                let position = self.offsets.wcs(*wcs);
                let mut values = [None, None, None];
                if let Some(position) = position {
                    for (index, axis) in [Axis::X, Axis::Y, Axis::Z].iter().enumerate() {
                        if let Some(core) = CoreAxis::from_index(axis.index()) {
                            values[index] = Some(self.units.display(position.get(core)));
                        }
                    }
                }
                WcsRow {
                    wcs: *wcs,
                    active: wcs.index() == self.active_wcs,
                    reported: position.is_some(),
                    values,
                }
            })
            .collect()
    }

    /// One WCS row by index.
    #[must_use]
    pub fn wcs_row(&self, index: usize) -> Option<WcsRow> {
        self.wcs_rows().into_iter().nth(index)
    }

    /// Stored `$#` value for a work coordinate system, in millimetres.
    #[must_use]
    pub fn wcs_position(&self, wcs: Wcs) -> Option<Position> {
        self.offsets.wcs(wcs)
    }

    /// Reported `G92` offset, in millimetres.
    #[must_use]
    pub fn g92_position(&self) -> Option<Position> {
        self.offsets.g92
    }

    /// Active tool-length offset on Z, in millimetres.
    ///
    /// A one-value `[TLO:...]` line is interpreted as Z, matching grblHAL's
    /// single-axis tool-length convention. A multi-value line uses the Z
    /// entry. An empty or absent line is zero.
    #[must_use]
    pub fn tlo_z_mm(&self) -> f64 {
        match self.offsets.tlo.as_deref() {
            Some([single]) => *single,
            Some(values) => values.get(2).copied().unwrap_or(0.0),
            None => 0.0,
        }
    }

    /// Read-only rows for the remaining `$#` parameters.
    #[must_use]
    pub fn named_rows(&self) -> Vec<NamedRow> {
        let mut rows = Vec::new();
        if let Some(position) = self.offsets.g28 {
            rows.push(NamedRow {
                label: "G28",
                value: self.position_text(position),
                note: None,
            });
        }
        if let Some(position) = self.offsets.g30 {
            rows.push(NamedRow {
                label: "G30",
                value: self.position_text(position),
                note: None,
            });
        }
        if let Some(position) = self.offsets.g92 {
            rows.push(NamedRow {
                label: "G92",
                value: self.position_text(position),
                note: Some("NON-PERSISTENT"),
            });
        }
        if let Some(position) = self.offsets.home {
            rows.push(NamedRow {
                label: "HOME",
                value: self.position_text(position),
                note: self
                    .offsets
                    .home_mask
                    .map(|_| "MACHINE")
                    .or(Some("REPORTED")),
            });
        }
        if let Some(tlo) = &self.offsets.tlo {
            rows.push(NamedRow {
                label: "TLO",
                value: self.tool_values_text(tlo),
                note: None,
            });
        }
        if let Some(tlr) = self.offsets.tlr {
            rows.push(NamedRow {
                label: "TLR",
                value: format!("{:.dec$}", self.units.display(tlr), dec = self.decimals()),
                note: None,
            });
        }
        if let Some(scaling) = self.offsets.scaling {
            rows.push(NamedRow {
                label: "G51",
                value: scaling_text(scaling),
                note: Some("SCALE"),
            });
        }
        if let Some(probe) = self.offsets.probe {
            rows.push(probe_row(probe, self.units));
        }
        rows
    }

    /// Build a `G10 L20` line that sets the current machine position to
    /// `value` in `wcs` for one axis.
    ///
    /// `value` is in display units, matching the controller's active parser
    /// units. The line never contains a motion word.
    ///
    /// # Errors
    ///
    /// Returns a message for an unsupported axis, a non-finite value, or a
    /// value outside [`MAX_OFFSET_MM`].
    pub fn g10_command(&self, wcs: Wcs, axis: Axis, value: f64) -> Result<String, String> {
        if !self.axes.contains(&axis) {
            return Err(format!("the {} axis is not reported", axis.label()));
        }
        let value_mm = self.units.to_mm(value);
        if !value_mm.is_finite() {
            return Err("the value must be finite".to_string());
        }
        if value_mm.abs() > MAX_OFFSET_MM {
            return Err(format!(
                "the value must stay within +/-{MAX_OFFSET_MM:.0} mm"
            ));
        }
        if wcs.index() >= Wcs::ALL.len() {
            return Err("invalid work coordinate system".to_string());
        }
        Ok(format!(
            "G10 L20 P{} {}{:.dec$}",
            wcs.index() + 1,
            axis.label(),
            value,
            dec = self.decimals()
        ))
    }

    /// Zero one axis in `wcs`.
    ///
    /// # Errors
    ///
    /// See [`OffsetModel::g10_command`].
    pub fn zero_axis_command(&self, wcs: Wcs, axis: Axis) -> Result<String, String> {
        self.g10_command(wcs, axis, 0.0)
    }

    /// Zero every reported axis in `wcs` on one line.
    ///
    /// # Errors
    ///
    /// Returns a message when no axis is available or the formatted line is
    /// empty.
    pub fn zero_all_command(&self, wcs: Wcs) -> Result<String, String> {
        if self.axes.is_empty() {
            return Err("no axis is reported".to_string());
        }
        let mut line = format!("G10 L20 P{}", wcs.index() + 1);
        for axis in &self.axes {
            line.push(' ');
            line.push_str(axis.label());
            line.push_str(&format!("{:.dec$}", 0.0, dec = self.decimals()));
        }
        Ok(line)
    }

    /// Capture the current live offsets for a local profile or backup.
    #[must_use]
    pub fn snapshot(&self) -> OffsetSnapshot {
        let mut snapshot = OffsetSnapshot::new();
        for wcs in Wcs::ALL {
            if let Some(position) = self.offsets.wcs(wcs) {
                snapshot.set_work_offset(wcs, position);
            }
        }
        snapshot.g28 = self.offsets.g28;
        snapshot.g30 = self.offsets.g30;
        snapshot.g92 = self.offsets.g92;
        snapshot.home = self.offsets.home;
        if let Some(tlo) = &self.offsets.tlo {
            let axes: Vec<CoreAxis> = CoreAxis::ALL.into_iter().take(tlo.len()).collect();
            snapshot.tool_length = Position::from_values(Axes::from_iter(axes), tlo);
        }
        if let Some(tlr) = self.offsets.tlr {
            // `TLR` is a single reference value; store it on the first axis so
            // the scalar survives a round trip without inventing Y/Z values.
            let mut position = Position::zero();
            position.set(CoreAxis::X, tlr);
            snapshot.tool_radius = Some(position);
        }
        snapshot.scaling = self
            .offsets
            .scaling
            .map(|position| position.iter().map(|(_, value)| value).collect());
        snapshot
    }

    /// Format a display value with the unit's decimal count.
    #[must_use]
    pub fn format_value(&self, value: f64) -> String {
        format!("{:.dec$}", value, dec = self.decimals())
    }

    /// Format one axis value, or `--`.
    #[must_use]
    pub fn format_optional(&self, value: Option<f64>) -> String {
        value.map_or_else(|| "--".to_string(), |value| self.format_value(value))
    }

    fn decimals(&self) -> usize {
        match self.units {
            Units::Millimetre => 3,
            Units::Inch => 4,
        }
    }

    fn position_text(&self, position: Position) -> String {
        let mut parts = Vec::new();
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let Some(core) = CoreAxis::from_index(axis.index()) else {
                continue;
            };
            parts.push(format!(
                "{}{}",
                axis.label(),
                self.format_value(self.units.display(position.get(core)))
            ));
        }
        parts.join("  ")
    }

    fn tool_values_text(&self, values: &[f64]) -> String {
        if values.len() == 1 {
            return format!("Z{}", self.format_value(self.units.display(values[0])));
        }
        let mut parts = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let label = Axis::from_index(index).label();
            parts.push(format!(
                "{label}{}",
                self.format_value(self.units.display(*value))
            ));
        }
        parts.join("  ")
    }
}

fn probe_row(probe: ProbeResult, units: Units) -> NamedRow {
    let mut parts = Vec::new();
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let Some(core) = CoreAxis::from_index(axis.index()) else {
            continue;
        };
        parts.push(format!(
            "{}{:.dec$}",
            axis.label(),
            units.display(probe.position.get(core)),
            dec = match units {
                Units::Millimetre => 3,
                Units::Inch => 4,
            }
        ));
    }
    NamedRow {
        label: "PRB",
        value: parts.join("  "),
        note: Some(if probe.success { "HIT" } else { "NO HIT" }),
    }
}

fn scaling_text(position: Position) -> String {
    let mut parts = Vec::new();
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let Some(core) = CoreAxis::from_index(axis.index()) else {
            continue;
        };
        parts.push(format!("{}{:.4}", axis.label(), position.get(core)));
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(units: Units) -> OffsetModel {
        let mut model = OffsetModel::default();
        model.set_context(units, &[Axis::X, Axis::Y, Axis::Z], 0);
        model
    }

    fn sample_offsets() -> Offsets {
        let mut offsets = Offsets::default();
        offsets.wcs[Wcs::G54.index()] = Some(Position::xyz(-357.0, -757.0, -148.306));
        offsets.wcs[Wcs::G59_3.index()] = Some(Position::xyz(1.5, -2.5, 3.5));
        offsets.g28 = Some(Position::xyz(0.0, 0.0, -2.0));
        offsets.g30 = Some(Position::xyz(1.0, 1.0, 1.0));
        offsets.g92 = Some(Position::xyz(0.5, 0.0, 0.0));
        offsets.home = Some(Position::xyz(-299.0, -353.0, -2.0));
        offsets.tlo = Some(vec![0.0, 0.0, 3.0]);
        offsets.tlr = Some(12.5);
        offsets.scaling = Some(Position::xyz(1.0, 2.0, 3.0));
        offsets.probe = Some(ProbeResult::new(true, Position::xyz(-1.0, 2.0, -3.0)));
        offsets
    }

    #[test]
    fn wcs_rows_keep_order_and_map_the_active_system() {
        let mut model = model(Units::Millimetre);
        model.set_context(
            Units::Millimetre,
            &[Axis::X, Axis::Y, Axis::Z],
            Wcs::G56.index(),
        );
        model.apply_engine(&sample_offsets());

        let rows = model.wcs_rows();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0].wcs, Wcs::G54);
        assert_eq!(rows[8].wcs, Wcs::G59_3);
        assert!(rows[2].active);
        assert!(!rows[0].active);
        assert!(rows[0].reported);
        assert!(!rows[1].reported);
        assert_eq!(rows[0].values[0], Some(-357.0));
        assert_eq!(rows[0].values[1], Some(-757.0));
        assert_eq!(rows[0].values[2], Some(-148.306));
        assert_eq!(rows[1].values, [None, None, None]);
        assert_eq!(model.format_optional(rows[1].values[0]), "--");
    }

    #[test]
    fn inch_display_converts_and_formats_with_four_decimals() {
        let mut model = model(Units::Inch);
        model.apply_engine(&sample_offsets());
        let row = model.wcs_row(0).expect("G54");
        assert!((row.values[0].expect("X") - (-357.0 / 25.4)).abs() < 1.0e-9);
        assert_eq!(model.format_value(1.0), "1.0000");

        let line = model
            .g10_command(Wcs::G54, Axis::X, 1.0)
            .expect("valid line");
        assert_eq!(line, "G10 L20 P1 X1.0000");
    }

    #[test]
    fn g10_maps_every_wcs_and_zeros_without_motion_words() {
        let model = model(Units::Millimetre);
        for wcs in Wcs::ALL {
            let line = model.zero_axis_command(wcs, Axis::Z).expect("line");
            assert_eq!(line, format!("G10 L20 P{} Z0.000", wcs.index() + 1));
            assert!(!line.contains("G0"));
            assert!(!line.contains("G1 "));
        }
        assert_eq!(
            model.zero_all_command(Wcs::G55).expect("line"),
            "G10 L20 P2 X0.000 Y0.000 Z0.000"
        );
    }

    #[test]
    fn invalid_offset_values_are_rejected() {
        let model = model(Units::Millimetre);
        assert!(model.g10_command(Wcs::G54, Axis::X, f64::NAN).is_err());
        assert!(
            model
                .g10_command(Wcs::G54, Axis::X, MAX_OFFSET_MM + 1.0)
                .is_err()
        );
        let mut two_axis = OffsetModel::default();
        two_axis.set_context(Units::Millimetre, &[Axis::X, Axis::Y], 0);
        assert!(two_axis.g10_command(Wcs::G54, Axis::Z, 0.0).is_err());
    }

    #[test]
    fn named_rows_format_offsets_and_probe() {
        let mut model = model(Units::Millimetre);
        model.apply_engine(&sample_offsets());
        let rows = model.named_rows();
        let by_label = |label: &str| rows.iter().find(|row| row.label == label);
        assert!(by_label("G28").is_some());
        assert!(by_label("G30").is_some());
        assert_eq!(
            by_label("G92").and_then(|row| row.note),
            Some("NON-PERSISTENT")
        );
        assert!(by_label("HOME").is_some());
        assert_eq!(
            by_label("TLO").map(|row| row.value.clone()),
            Some("X0.000  Y0.000  Z3.000".to_string())
        );
        assert_eq!(
            by_label("TLR").map(|row| row.value.clone()),
            Some("12.500".to_string())
        );
        assert_eq!(
            by_label("G51").map(|row| row.value.clone()),
            Some("X1.0000  Y2.0000  Z3.0000".to_string())
        );
        let probe = by_label("PRB").expect("probe row");
        assert_eq!(probe.note, Some("HIT"));
        assert!(probe.value.contains("X-1.000"));
    }

    #[test]
    fn snapshot_round_trips_the_reported_fields() {
        let mut model = model(Units::Millimetre);
        model.apply_engine(&sample_offsets());
        let snapshot = model.snapshot();
        assert_eq!(
            snapshot.work_offset(Wcs::G54),
            Some(Position::xyz(-357.0, -757.0, -148.306))
        );
        assert_eq!(snapshot.g28, Some(Position::xyz(0.0, 0.0, -2.0)));
        assert_eq!(snapshot.g92, Some(Position::xyz(0.5, 0.0, 0.0)));
        assert_eq!(snapshot.tool_length, Some(Position::xyz(0.0, 0.0, 3.0)));
        assert_eq!(
            snapshot.tool_radius.map(|pos| pos.get(CoreAxis::X)),
            Some(12.5)
        );
        assert_eq!(snapshot.scaling, Some(vec![1.0, 2.0, 3.0]));
        snapshot.validate().expect("snapshot is finite");
    }

    #[test]
    fn refresh_status_tracks_the_read_back() {
        let mut model = model(Units::Millimetre);
        assert!(!model.reported());
        model.command_sent("G10 L20 P1 X0.000");
        assert_eq!(model.status(), &OffsetStatus::Refreshing);
        model.apply_engine(&sample_offsets());
        assert_eq!(model.status(), &OffsetStatus::Refreshed);
        assert!(model.reported());
        model.failed("error:20");
        assert_eq!(
            model.status(),
            &OffsetStatus::Failed("error:20".to_string())
        );
    }
}
