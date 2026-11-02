//! Saved per-controller height maps.
//!
//! This module is deliberately pure: it converts a finished probe run into the
//! persistence-only [`HeightMapRecord`] owned by `ghs-config`, converts a stored
//! record back into the probe/viewer types, and tracks which saved map the
//! operator is currently viewing. It never touches a serial port, the engine,
//! GPUI, or a clock: created-at timestamps are supplied by the caller.
//!
//! Loading a saved record only builds a read-only overlay and a
//! [`HeightMapSnapshot`](crate::height_compensation::HeightMapSnapshot). It does
//! not apply compensation and does not load or start a program-keyed job.

use std::path::PathBuf;

use ghs_config::{HeightMapFrame, HeightMapRecord};
use ghs_engine::{Axis as CoreAxis, Position, Wcs};
use ghs_probe::{HeightMap, HeightMapGrid, XyPoint};

use crate::height_compensation::{HeightMapSnapshot, OffsetFrame};
use crate::probe::{
    HeightMapSummary, ProbeContext, ProbeRun, ProbeSetup, height_map_overlay, height_map_summary,
};
use crate::viewer::HeightOverlay;

/// Compact list row for one stored map.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedMapSummary {
    /// User label, unique only by name replacement on save.
    pub name: String,
    /// Creation time as Unix seconds, when the record carries one.
    pub created_at_unix_s: Option<u64>,
    /// Total grid cells.
    pub point_count: usize,
    /// Cells with a measured Z.
    pub valid: usize,
    /// Cells without a valid measurement.
    pub missing: usize,
    /// Every cell has a measurement.
    pub complete: bool,
    /// Work coordinate system used while probing.
    pub wcs: Wcs,
    /// Tool number recorded with the map, when known.
    pub tool_number: Option<u32>,
}

impl SavedMapSummary {
    /// Build a compact row from a stored record without converting the samples.
    #[must_use]
    pub fn from_record(record: &HeightMapRecord) -> Self {
        let point_count = record.point_count();
        let valid = record.samples.iter().flatten().count();
        Self {
            name: record.name.clone(),
            created_at_unix_s: record.created_at_unix_s,
            point_count,
            valid,
            missing: point_count.saturating_sub(valid),
            complete: valid == point_count,
            wcs: record.frame.wcs,
            tool_number: record.tool_number,
        }
    }
}

/// One stored map converted back into the app's live types.
#[derive(Clone, Debug)]
pub struct LoadedSavedMap {
    /// Original record, kept for re-saving under a new name and for the frame.
    pub record: HeightMapRecord,
    /// Probe map rebuilt from the row-major samples.
    pub map: HeightMap,
    /// Counts and Z/compensation ranges.
    pub summary: HeightMapSummary,
    /// Overlay projected into the record's work frame.
    pub overlay: HeightOverlay,
    /// Measurement frame from the record.
    pub frame: OffsetFrame,
}

impl LoadedSavedMap {
    /// Validate a record and rebuild its grid, samples, summary, and overlay.
    ///
    /// # Errors
    ///
    /// Returns a message when validation fails or the rebuilt grid is rejected
    /// by `ghs-probe`.
    pub fn from_record(record: HeightMapRecord) -> Result<Self, String> {
        record
            .validate()
            .map_err(|error| format!("saved map '{}' is invalid: {error}", record.name))?;
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(record.origin_x_mm, record.origin_y_mm),
            step_x_mm: record.step_x_mm,
            step_y_mm: record.step_y_mm,
            columns: record.columns,
            rows: record.rows,
            surface_z_mm: record.surface_z_mm,
        };
        let map = HeightMap::from_samples(grid, record.samples.clone()).map_err(|error| {
            format!("saved map '{}' could not be rebuilt: {error}", record.name)
        })?;
        let frame = OffsetFrame {
            wcs: record.frame.wcs,
            wcs_value_mm: record.frame.wcs_offset,
            g92_mm: record.frame.g92,
            tlo_z_mm: record.frame.tool_length_offset_z_mm,
        };
        let summary = height_map_summary(&map);
        let overlay = height_map_overlay(&map, frame.offset_mm());
        Ok(Self {
            record,
            map,
            summary,
            overlay,
            frame,
        })
    }

    /// Snapshot accepted by the existing explicit Apply/Revert workflow.
    #[must_use]
    pub fn snapshot(&self) -> HeightMapSnapshot {
        HeightMapSnapshot {
            complete: self.map.is_complete(),
            frame: self.frame,
            map: self.map.clone(),
        }
    }

    /// Frame differences between this saved map and the live controller.
    ///
    /// Returns one message per changed WCS, WCS offset, G92, or TLO Z, so the
    /// UI can state exactly why a saved map must not be used as-is.
    #[must_use]
    pub fn frame_mismatches(&self, live: &ProbeContext) -> Vec<String> {
        self.frame.mismatches(&OffsetFrame::from_context(live))
    }
}

/// Convert a finished probe run into the persistence record.
///
/// The map's own grid is authoritative for samples; the run's setup supplies
/// the measurement frame. A partial map is accepted as long as at least one
/// cell has a valid measurement. Missing cells remain `None`.
///
/// # Errors
///
/// Returns a message when the run has no height map, has no valid cell, is not
/// a height-map run, or the resulting record fails `ghs-config` validation.
pub fn record_from_run(
    run: &ProbeRun,
    name: &str,
    created_at_unix_s: Option<u64>,
    tool_number: Option<u32>,
) -> Result<HeightMapRecord, String> {
    if !run.is_height_map() {
        return Err("this probe run is not a height map".to_string());
    }
    let map = run
        .map
        .as_ref()
        .ok_or_else(|| "the probe run has no measured height map".to_string())?;
    if map.valid_count() == 0 {
        return Err("the height map has no valid cell to save".to_string());
    }
    let grid = map.grid();
    record_from_parts(
        grid,
        map.samples(),
        &run.setup,
        name,
        created_at_unix_s,
        tool_number,
    )
}

fn record_from_parts(
    grid: &HeightMapGrid,
    samples: &[Option<f64>],
    setup: &ProbeSetup,
    name: &str,
    created_at_unix_s: Option<u64>,
    tool_number: Option<u32>,
) -> Result<HeightMapRecord, String> {
    let record = HeightMapRecord {
        name: name.trim().to_string(),
        tool_number,
        created_at_unix_s,
        origin_x_mm: grid.origin_mm.x_mm,
        origin_y_mm: grid.origin_mm.y_mm,
        step_x_mm: grid.step_x_mm,
        step_y_mm: grid.step_y_mm,
        columns: grid.columns,
        rows: grid.rows,
        surface_z_mm: grid.surface_z_mm,
        // The probe workflow records no separate constant Z offset.
        z_offset_mm: None,
        samples: samples.to_vec(),
        frame: HeightMapFrame {
            wcs: setup.params.wcs,
            wcs_offset: xyz_position(setup.wcs_value_mm),
            g92: xyz_position(setup.g92_mm),
            tool_length_offset_z_mm: setup.tlo_z_mm,
        },
    };
    record
        .validate()
        .map_err(|error| format!("height map is not saveable: {error}"))?;
    Ok(record)
}

/// Keep X/Y/Z and drop any coordinates outside the three-axis app model.
fn xyz_position(position: Position) -> Position {
    Position::xyz(
        position.get(CoreAxis::X),
        position.get(CoreAxis::Y),
        position.get(CoreAxis::Z),
    )
}

/// Readable default label for a new saved map.
///
/// UTC is used because the app has no timezone service; the suffix makes that
/// explicit in the profile. Seconds keep two saves in the same minute from
/// silently colliding on the replacement rule.
#[must_use]
pub fn default_map_name(unix_s: u64) -> String {
    format!("Map {}", format_unix_utc_seconds(unix_s))
}

/// `YYYY-MM-DD HH:MM UTC` for a Unix timestamp.
#[must_use]
pub fn format_unix_utc(unix_s: u64) -> String {
    let (year, month, day, hour, minute, _) = utc_parts(unix_s);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// `YYYY-MM-DD HH:MM:SS UTC` for a Unix timestamp.
#[must_use]
pub fn format_unix_utc_seconds(unix_s: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(unix_s);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn utc_parts(unix_s: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = i64::try_from(unix_s / 86_400).unwrap_or(i64::MAX);
    let seconds = unix_s % 86_400;
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60,
    )
}

/// Convert days since 1970-01-01 to a proleptic Gregorian date.
///
/// Howard Hinnant's `civil_from_days`; no external date dependency.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// Result of the last successful save, shown beside the profile path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedMapNotice {
    /// Record label written.
    pub name: String,
    /// Profile file that received the record.
    pub path: PathBuf,
}

/// Read-only selection state for the height-map workspace.
#[derive(Clone, Debug, Default)]
pub struct HeightMapLibrary {
    selected: Option<LoadedSavedMap>,
    last_saved: Option<SavedMapNotice>,
    last_error: Option<String>,
}

impl HeightMapLibrary {
    /// Clear the current selection and any save feedback.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Drop the current selection, leaving save feedback in place.
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Currently viewed saved map.
    #[must_use]
    pub fn selected(&self) -> Option<&LoadedSavedMap> {
        self.selected.as_ref()
    }

    /// Select and convert one stored record for display.
    ///
    /// This only builds a view model; it never applies compensation, loads a
    /// program, or starts a job.
    ///
    /// # Errors
    ///
    /// Returns the validation/rebuild message from
    /// [`LoadedSavedMap::from_record`].
    pub fn select(&mut self, record: HeightMapRecord) -> Result<(), String> {
        match LoadedSavedMap::from_record(record) {
            Ok(loaded) => {
                self.selected = Some(loaded);
                self.last_error = None;
                Ok(())
            }
            Err(message) => {
                self.selected = None;
                self.last_error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Record a successful profile save.
    pub fn mark_saved(&mut self, name: impl Into<String>, path: PathBuf) {
        self.last_saved = Some(SavedMapNotice {
            name: name.into(),
            path,
        });
        self.last_error = None;
    }

    /// Record a refused or failed save.
    pub fn mark_error(&mut self, message: impl Into<String>) {
        self.last_saved = None;
        self.last_error = Some(message.into());
    }

    /// Last successful save.
    #[must_use]
    pub fn last_saved(&self) -> Option<&SavedMapNotice> {
        self.last_saved.as_ref()
    }

    /// Last save/selection error.
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use ghs_probe::{CycleKind, ProbeParams, ProbePlan, ProbePreconditions};

    use super::*;
    use crate::probe::{ProbePhase, ProbeTarget};

    fn grid() -> HeightMapGrid {
        HeightMapGrid {
            origin_mm: XyPoint::new(10.0, 20.0),
            step_x_mm: 5.0,
            step_y_mm: 2.5,
            columns: 3,
            rows: 2,
            surface_z_mm: -1.0,
        }
    }

    fn frame() -> OffsetFrame {
        OffsetFrame {
            wcs: Wcs::G55,
            wcs_value_mm: Position::xyz(1.0, 2.0, 3.0),
            g92_mm: Position::xyz(0.1, 0.2, 0.3),
            tlo_z_mm: -12.5,
        }
    }

    fn sample_map() -> HeightMap {
        HeightMap::from_samples(
            grid(),
            vec![
                Some(-1.0),
                Some(-1.1),
                None,
                Some(-1.2),
                Some(-1.3),
                Some(-1.4),
            ],
        )
        .expect("sample map")
    }

    fn sample_run(map: HeightMap, frame: &OffsetFrame) -> ProbeRun {
        let params = ProbeParams {
            wcs: frame.wcs,
            wcs_offset_mm: frame.wcs_value_mm + frame.g92_mm,
            ..ProbeParams::default()
        };
        let plan = ProbePlan {
            name: "height-map".to_string(),
            kind: CycleKind::HeightMap,
            program: String::new(),
            probes: Vec::new(),
            expected_probes: map.len(),
            preconditions: ProbePreconditions::from_params(&params),
            wcs: frame.wcs,
            safe_z_mm: params.safe_z_mm,
        };
        ProbeRun {
            setup: ProbeSetup {
                params,
                target: ProbeTarget::HeightMap(map.grid().clone()),
                plan,
                estimate: None,
                wcs_value_mm: frame.wcs_value_mm,
                g92_mm: frame.g92_mm,
                tlo_z_mm: frame.tlo_z_mm,
            },
            results: Vec::new(),
            raw: Vec::new(),
            outcome: None,
            map: Some(map),
            export: None,
            error: None,
            phase: ProbePhase::Complete,
            loaded: true,
            started: true,
            unexpected_results: 0,
            apply_confirm: false,
            kept: false,
            applied_line: None,
            checkpoints: Vec::new(),
            paused_after_probe: None,
            stop_requested: false,
        }
    }

    #[test]
    fn record_conversion_round_trips_grid_samples_frame_tool_and_name() {
        let run = sample_run(sample_map(), &frame());
        let record =
            record_from_run(&run, "  Fixture top  ", Some(1_700_000_000), Some(3)).expect("record");

        assert_eq!(record.name, "Fixture top");
        assert_eq!(record.created_at_unix_s, Some(1_700_000_000));
        assert_eq!(record.tool_number, Some(3));
        assert_eq!(record.origin_x_mm, 10.0);
        assert_eq!(record.origin_y_mm, 20.0);
        assert_eq!(record.step_x_mm, 5.0);
        assert_eq!(record.step_y_mm, 2.5);
        assert_eq!(record.columns, 3);
        assert_eq!(record.rows, 2);
        assert_eq!(record.surface_z_mm, -1.0);
        assert_eq!(record.z_offset_mm, None);
        assert_eq!(
            record.samples,
            vec![
                Some(-1.0),
                Some(-1.1),
                None,
                Some(-1.2),
                Some(-1.3),
                Some(-1.4)
            ]
        );
        assert_eq!(record.frame.wcs, Wcs::G55);
        assert_eq!(record.frame.wcs_offset, Position::xyz(1.0, 2.0, 3.0));
        assert_eq!(record.frame.g92, Position::xyz(0.1, 0.2, 0.3));
        assert_eq!(record.frame.tool_length_offset_z_mm, -12.5);
        record.validate().expect("valid record");

        let loaded = LoadedSavedMap::from_record(record.clone()).expect("rebuilt");
        assert_eq!(loaded.map.grid(), &grid());
        assert_eq!(loaded.map.samples(), record.samples.as_slice());
        assert_eq!(loaded.frame, frame());
        assert_eq!(loaded.summary.valid, 5);
        assert_eq!(loaded.summary.missing, 1);
        assert!(!loaded.summary.complete);
        assert_eq!(loaded.overlay.cells.len(), 6);
        assert_eq!(LoadedSavedMap::from_record(record).unwrap().map, loaded.map);
    }

    #[test]
    fn frame_mismatch_names_each_changed_element_and_match_is_silent() {
        let loaded = LoadedSavedMap::from_record(
            record_from_run(&sample_run(sample_map(), &frame()), "top", None, None)
                .expect("record"),
        )
        .expect("loaded");

        let context = ProbeContext {
            live: true,
            ready: true,
            machine_state: ghs_engine::MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::zero(),
            work_pos_mm: Position::zero(),
            wcs: frame().wcs,
            wcs_value_mm: frame().wcs_value_mm,
            g92_mm: frame().g92_mm,
            tlo_z_mm: frame().tlo_z_mm,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: crate::state::Units::Millimetre,
        };
        assert!(loaded.frame_mismatches(&context).is_empty());

        let mut moved = context.clone();
        moved.wcs = Wcs::G54;
        moved.wcs_value_mm = Position::xyz(9.0, 8.0, 7.0);
        moved.g92_mm = Position::xyz(0.0, 0.0, 0.0);
        moved.tlo_z_mm = 0.0;
        let messages = loaded.frame_mismatches(&moved);
        assert_eq!(messages.len(), 4, "{messages:?}");
        assert!(messages.iter().any(|text| text.contains("WCS changed")));
        assert!(
            messages
                .iter()
                .any(|text| text.contains("WCS offset changed"))
        );
        assert!(
            messages
                .iter()
                .any(|text| text.contains("G92 offset changed"))
        );
        assert!(
            messages
                .iter()
                .any(|text| text.contains("tool-length offset changed"))
        );
    }

    #[test]
    fn empty_and_non_height_runs_are_refused() {
        let empty = HeightMap::from_samples(grid(), vec![None; 6]).expect("empty map");
        let error = record_from_run(&sample_run(empty, &frame()), "empty", None, None)
            .expect_err("no valid cells");
        assert!(error.contains("no valid cell"), "{error}");

        let mut run = sample_run(sample_map(), &frame());
        run.map = None;
        assert!(record_from_run(&run, "none", None, None).is_err());
    }

    #[test]
    fn utc_labels_are_readable_and_deterministic() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_utc(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(format_unix_utc_seconds(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(default_map_name(0), "Map 1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn summaries_count_missing_cells_without_filling_them() {
        let record =
            record_from_run(&sample_run(sample_map(), &frame()), "top", None, Some(7)).unwrap();
        let summary = SavedMapSummary::from_record(&record);
        assert_eq!(summary.point_count, 6);
        assert_eq!(summary.valid, 5);
        assert_eq!(summary.missing, 1);
        assert!(!summary.complete);
        assert_eq!(summary.wcs, Wcs::G55);
        assert_eq!(summary.tool_number, Some(7));
    }
}
