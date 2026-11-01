//! Probe workspace model.
//!
//! This module owns the preflight inputs, the `ghs-probe` plan/result binding,
//! and the `G10 L2` offset commands the shell sends after an explicit Apply.
//! It holds no GPUI types and never talks to a transport, so the safety gates,
//! unit conversion, correlation and command formatting are testable without a
//! window or an engine.
//!
//! # Frame conventions
//!
//! All probe fields and rows are millimetres. Cycle geometry and every
//! `ProbeResult` are machine coordinates, matching `PRB`. The active work
//! offset used for the generated program is
//! `WCS + G92 + TLO` (`machine = work + offset`): `TLO` is included because the
//! plan never emits `G49`, and the preflight shows it explicitly. `G10 L2`
//! writes the stored WCS value only, so an Apply subtracts `G92` and `TLO`
//! from the measured machine target; `G92` stays untouched and is displayed as
//! non-persistent.
//!
//! # Safety
//!
//! [`ProbeModel::preflight`] refuses to build a runnable setup unless the link
//! is ready, the machine is Idle, no job is active, the spindle and coolant are
//! off, the probe is verified (PC signal when reported, otherwise an explicit
//! operator checkbox), the probe is not already triggered, and `$#` has
//! answered so the WCS conversion is real. The engine safety gateway remains
//! the final authority; nothing here sends motion.

use ghs_core::Units;
use ghs_engine::{Axis as CoreAxis, MachineState, Position, ProbeResult, Wcs};
use ghs_probe::{
    CornerProbe, EdgeMode, EdgeSpec, MAX_CIRCLE_TOUCHES, MAX_COORDINATE_MM, MAX_EDGE_OFFSET_MM,
    MAX_PLATE_THICKNESS_MM, MAX_PROBE_DIAMETER_MM, MAX_PROBE_FEED_MM_MIN, MAX_RADIUS_MM,
    MAX_RETRACT_MM, MAX_SEARCH_DISTANCE_MM, MAX_SEPARATION_MM, MIN_CIRCLE_TOUCHES, ProbeAxis,
    ProbeCycle, ProbeOutcome, ProbeParams, ProbePlan, ProbeValue, RotationProbe, Sign,
};

use crate::state::Units as DisplayUnits;

/// One editable numeric preflight field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeField {
    /// Safe Z plane, in machine millimetres.
    SafeZ,
    /// Maximum search travel, in millimetres.
    Search,
    /// Probe feed, in millimetres per minute.
    Feed,
    /// Relative retract after a touch, in millimetres.
    Retract,
    /// Probe tip diameter, in millimetres.
    Diameter,
    /// Distance from the measured edge into the material.
    EdgeOffset,
    /// Plate thickness subtracted from a Z contact.
    PlateThickness,
    /// Expected plate/surface Z, in machine millimetres.
    ExpectedZ,
    /// XY probing height, in machine millimetres.
    ProbingHeight,
    /// Expected X edge, in machine millimetres.
    ExpectedX,
    /// Expected Y edge, in machine millimetres.
    ExpectedY,
    /// Approximate circle centre X, in machine millimetres.
    CenterX,
    /// Approximate circle centre Y, in machine millimetres.
    CenterY,
    /// Approximate circle radius, in millimetres.
    Radius,
    /// Number of circle touches.
    Touches,
    /// Rotation point separation, in millimetres.
    Separation,
    /// Tool-length reference surface Z, in machine millimetres.
    ReferenceZ,
}

impl ProbeField {
    /// Every field, in edit order.
    pub const ALL: [ProbeField; 17] = [
        ProbeField::SafeZ,
        ProbeField::Search,
        ProbeField::Feed,
        ProbeField::Retract,
        ProbeField::Diameter,
        ProbeField::EdgeOffset,
        ProbeField::PlateThickness,
        ProbeField::ExpectedZ,
        ProbeField::ProbingHeight,
        ProbeField::ExpectedX,
        ProbeField::ExpectedY,
        ProbeField::CenterX,
        ProbeField::CenterY,
        ProbeField::Radius,
        ProbeField::Touches,
        ProbeField::Separation,
        ProbeField::ReferenceZ,
    ];

    /// Stable zero-based index for the input array.
    pub const fn index(self) -> usize {
        match self {
            ProbeField::SafeZ => 0,
            ProbeField::Search => 1,
            ProbeField::Feed => 2,
            ProbeField::Retract => 3,
            ProbeField::Diameter => 4,
            ProbeField::EdgeOffset => 5,
            ProbeField::PlateThickness => 6,
            ProbeField::ExpectedZ => 7,
            ProbeField::ProbingHeight => 8,
            ProbeField::ExpectedX => 9,
            ProbeField::ExpectedY => 10,
            ProbeField::CenterX => 11,
            ProbeField::CenterY => 12,
            ProbeField::Radius => 13,
            ProbeField::Touches => 14,
            ProbeField::Separation => 15,
            ProbeField::ReferenceZ => 16,
        }
    }

    /// Upper-case label for the field cell.
    pub const fn label(self) -> &'static str {
        match self {
            ProbeField::SafeZ => "SAFE Z",
            ProbeField::Search => "SEARCH",
            ProbeField::Feed => "FEED",
            ProbeField::Retract => "RETRACT",
            ProbeField::Diameter => "PROBE DIA",
            ProbeField::EdgeOffset => "EDGE OFFSET",
            ProbeField::PlateThickness => "PLATE THICK",
            ProbeField::ExpectedZ => "EXPECTED Z",
            ProbeField::ProbingHeight => "PROBE HEIGHT",
            ProbeField::ExpectedX => "EXPECTED X",
            ProbeField::ExpectedY => "EXPECTED Y",
            ProbeField::CenterX => "CENTER X",
            ProbeField::CenterY => "CENTER Y",
            ProbeField::Radius => "RADIUS",
            ProbeField::Touches => "TOUCHES",
            ProbeField::Separation => "SEPARATION",
            ProbeField::ReferenceZ => "REFERENCE Z",
        }
    }

    /// Stable short key for element ids.
    pub const fn key(self) -> &'static str {
        match self {
            ProbeField::SafeZ => "safe-z",
            ProbeField::Search => "search",
            ProbeField::Feed => "feed",
            ProbeField::Retract => "retract",
            ProbeField::Diameter => "diameter",
            ProbeField::EdgeOffset => "edge-offset",
            ProbeField::PlateThickness => "plate-thickness",
            ProbeField::ExpectedZ => "expected-z",
            ProbeField::ProbingHeight => "probe-height",
            ProbeField::ExpectedX => "expected-x",
            ProbeField::ExpectedY => "expected-y",
            ProbeField::CenterX => "center-x",
            ProbeField::CenterY => "center-y",
            ProbeField::Radius => "radius",
            ProbeField::Touches => "touches",
            ProbeField::Separation => "separation",
            ProbeField::ReferenceZ => "reference-z",
        }
    }

    /// Unit suffix shown in the keypad hint.
    pub const fn unit(self) -> &'static str {
        match self {
            ProbeField::Touches => "",
            ProbeField::Feed => "mm/min",
            _ => "mm",
        }
    }

    /// Inclusive bounds applied while editing.
    pub const fn bounds(self) -> (f64, f64) {
        match self {
            ProbeField::Search => (0.0, MAX_SEARCH_DISTANCE_MM),
            ProbeField::Feed => (0.0, MAX_PROBE_FEED_MM_MIN),
            ProbeField::Retract => (0.0, MAX_RETRACT_MM),
            ProbeField::Diameter => (0.0, MAX_PROBE_DIAMETER_MM),
            ProbeField::EdgeOffset => (0.0, MAX_EDGE_OFFSET_MM),
            ProbeField::PlateThickness => (0.0, MAX_PLATE_THICKNESS_MM),
            ProbeField::Radius => (0.0, MAX_RADIUS_MM),
            ProbeField::Touches => (MIN_CIRCLE_TOUCHES as f64, MAX_CIRCLE_TOUCHES as f64),
            ProbeField::Separation => (0.0, MAX_SEPARATION_MM),
            _ => (-MAX_COORDINATE_MM, MAX_COORDINATE_MM),
        }
    }

    /// Factory text for the field when a card is opened.
    pub fn default_text(self) -> String {
        match self {
            ProbeField::SafeZ => "10.000".to_string(),
            ProbeField::Search => "5.000".to_string(),
            ProbeField::Feed => "100.000".to_string(),
            ProbeField::Retract => "2.000".to_string(),
            ProbeField::Diameter => "3.000".to_string(),
            ProbeField::EdgeOffset => "0.000".to_string(),
            ProbeField::PlateThickness => "10.000".to_string(),
            ProbeField::ExpectedZ => "-5.000".to_string(),
            ProbeField::ProbingHeight => "0.000".to_string(),
            ProbeField::ExpectedX => "20.000".to_string(),
            ProbeField::ExpectedY => "20.000".to_string(),
            ProbeField::CenterX => "0.000".to_string(),
            ProbeField::CenterY => "0.000".to_string(),
            ProbeField::Radius => "20.000".to_string(),
            ProbeField::Touches => "4".to_string(),
            ProbeField::Separation => "50.000".to_string(),
            ProbeField::ReferenceZ => "-5.000".to_string(),
        }
    }
}

/// One probe cycle card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeCard {
    /// Z touch plate.
    ZTouchPlate,
    /// X edge.
    EdgeX,
    /// Y edge.
    EdgeY,
    /// Two-edge corner.
    Corner,
    /// Circle centre.
    CenterCircle,
    /// Two-point rotation.
    Rotation,
    /// Tool-length reference.
    ToolLength,
    /// Height map.
    HeightMap,
}

impl ProbeCard {
    /// Every card, in display order.
    pub const ALL: [ProbeCard; 8] = [
        ProbeCard::ZTouchPlate,
        ProbeCard::EdgeX,
        ProbeCard::EdgeY,
        ProbeCard::Corner,
        ProbeCard::CenterCircle,
        ProbeCard::Rotation,
        ProbeCard::ToolLength,
        ProbeCard::HeightMap,
    ];

    /// Card title.
    pub const fn title(self) -> &'static str {
        match self {
            ProbeCard::ZTouchPlate => "Z touch plate",
            ProbeCard::EdgeX => "Edge X",
            ProbeCard::EdgeY => "Edge Y",
            ProbeCard::Corner => "Corner",
            ProbeCard::CenterCircle => "Center circle",
            ProbeCard::Rotation => "Rotation",
            ProbeCard::ToolLength => "Tool length",
            ProbeCard::HeightMap => "Height map",
        }
    }

    /// One-line explanation of the probe direction.
    pub const fn direction(self) -> &'static str {
        match self {
            ProbeCard::ZTouchPlate => "Down onto a plate; zero = plate top minus thickness.",
            ProbeCard::EdgeX => "Along X into the edge; radius + edge offset applied.",
            ProbeCard::EdgeY => "Along Y into the edge; radius + edge offset applied.",
            ProbeCard::Corner => "X edge then Y edge; sets both zeros at the corner.",
            ProbeCard::CenterCircle => "Radial touches around a bore/boss; fits centre + radius.",
            ProbeCard::Rotation => "Two points on an X edge; reports angle, apply deferred.",
            ProbeCard::ToolLength => "Down onto the reference; reports delta, TLO apply deferred.",
            ProbeCard::HeightMap => "Grid of Z touches; UI and compensation deferred.",
        }
    }

    /// Longer explanation for the detail view.
    pub const fn detail(self) -> &'static str {
        match self {
            ProbeCard::ZTouchPlate => {
                "Probes straight down onto a conductive plate on top of the stock; work Z is the \
                 plate contact minus the plate thickness."
            }
            ProbeCard::EdgeX => {
                "Rapid to safe Z, move to the X search start at the probing height, probe along X, \
                 then derive the edge and the offset work zero."
            }
            ProbeCard::EdgeY => {
                "Rapid to safe Z, move to the Y search start at the probing height, probe along Y, \
                 then derive the edge and the offset work zero."
            }
            ProbeCard::Corner => {
                "Probes the X edge first, retracts, then probes the Y edge at the same height. Both \
                 work zeros are computed from the measured edges and offsets."
            }
            ProbeCard::CenterCircle => {
                "Touches the bore or boss at evenly spaced angles, fits the contact circle, then \
                 corrects by the probe radius and reports the centre, radius and RMS error."
            }
            ProbeCard::Rotation => {
                "Probes two points on a nominally X-aligned edge and reports the measured angle and \
                 deviation. Applying work rotation is controller/app dependent and deferred."
            }
            ProbeCard::ToolLength => {
                "Probes down onto a reference surface and reports the signed tool-length delta. \
                 Applying TLO or `$TLR` is deferred until the tool-change workflow owns it."
            }
            ProbeCard::HeightMap => {
                "A bounded grid of Z touches with per-point validity and bilinear interpolation. The \
                 grid editor, progress sheet, overlay and compensation are not implemented here."
            }
        }
    }

    /// Whether this card is an explicit deferred state.
    pub const fn deferred_note(self) -> Option<&'static str> {
        match self {
            ProbeCard::HeightMap => Some(
                "Height mapping is deferred in this build. ghs-probe already has a bounded \
                 HeightMapGrid, per-point validity and bilinear interpolation, but this UI does \
                 not generate a grid, stream it, or draw/compensate a map. No fake map is shown.",
            ),
            ProbeCard::Rotation => Some(
                "The rotation cycle runs and reports the measured angle. Applying work rotation is \
                 controller/app dependent and is deferred; no G10 rotation word is sent.",
            ),
            ProbeCard::ToolLength => Some(
                "The tool-length cycle runs and reports the delta. Applying TLO/`$TLR` is deferred \
                 until the tool-change workflow owns it; no TLO write is sent from this card.",
            ),
            _ => None,
        }
    }

    /// Fields shown on this card.
    pub const fn fields(self) -> &'static [ProbeField] {
        match self {
            ProbeCard::ZTouchPlate => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::ExpectedZ,
                ProbeField::PlateThickness,
            ],
            ProbeCard::EdgeX => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::EdgeOffset,
                ProbeField::ProbingHeight,
                ProbeField::ExpectedX,
            ],
            ProbeCard::EdgeY => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::EdgeOffset,
                ProbeField::ProbingHeight,
                ProbeField::ExpectedY,
            ],
            ProbeCard::Corner => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::EdgeOffset,
                ProbeField::ProbingHeight,
                ProbeField::ExpectedX,
                ProbeField::ExpectedY,
            ],
            ProbeCard::CenterCircle => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::ProbingHeight,
                ProbeField::CenterX,
                ProbeField::CenterY,
                ProbeField::Radius,
                ProbeField::Touches,
            ],
            ProbeCard::Rotation => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::ProbingHeight,
                ProbeField::ExpectedY,
                ProbeField::Separation,
            ],
            ProbeCard::ToolLength => &[
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
                ProbeField::ReferenceZ,
            ],
            ProbeCard::HeightMap => &[],
        }
    }
}

/// Read-only machine/controller context for one preflight.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeContext {
    /// A live engine session is open.
    pub live: bool,
    /// The handshake completed.
    pub ready: bool,
    /// Current controller state.
    pub machine_state: MachineState,
    /// A job runner is feeding or paused.
    pub job_active: bool,
    /// Current machine position, in millimetres.
    pub machine_pos_mm: Position,
    /// Current work position, in millimetres.
    pub work_pos_mm: Position,
    /// Active work coordinate system.
    pub wcs: Wcs,
    /// Stored `$#` value for the active WCS, in millimetres.
    pub wcs_value_mm: Position,
    /// `$#` G92 offset, in millimetres.
    pub g92_mm: Position,
    /// Active tool-length offset Z, in millimetres.
    pub tlo_z_mm: f64,
    /// The spindle is on.
    pub spindle_on: bool,
    /// Any coolant channel is on.
    pub coolant_on: bool,
    /// The controller reports a probe input.
    pub has_probe: bool,
    /// The controller advertises a probe-connected (`PC`) signal.
    pub has_probe_connected_signal: bool,
    /// The `Pn` probe-disconnected bit is asserted.
    pub probe_disconnected: bool,
    /// The `Pn` probe-triggered bit is asserted.
    pub probe_triggered: bool,
    /// `$#` has answered at least once.
    pub offsets_reported: bool,
    /// Display unit for the surrounding DRO.
    pub units: DisplayUnits,
}

impl ProbeContext {
    /// Total offset that converts a work coordinate to machine:
    /// `WCS + G92 + TLO`.
    #[must_use]
    pub fn offset_mm(&self) -> Position {
        let mut tlo = Position::zero();
        tlo.set(CoreAxis::Z, self.tlo_z_mm);
        self.wcs_value_mm + self.g92_mm + tlo
    }

    /// Whether the probe connection is verified by the controller signal.
    #[must_use]
    pub fn pc_verified(&self) -> bool {
        self.has_probe_connected_signal && !self.probe_disconnected
    }
}

/// One completed preflight row.
#[derive(Clone, Debug, PartialEq)]
pub struct PreflightRow {
    /// Row label.
    pub label: &'static str,
    /// Formatted value.
    pub value: String,
    /// Severity for colouring.
    pub state: RowState,
}

/// Severity of one preflight row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// Verified and safe.
    Ok,
    /// Informational, not a gate.
    Info,
    /// A warning that does not block.
    Warn,
    /// A blocker; the run is refused.
    Bad,
}

/// A plan plus the offset snapshot it was built against.
#[derive(Clone, Debug)]
pub struct ProbeSetup {
    /// Shared parameters.
    pub params: ProbeParams,
    /// Cycle geometry.
    pub cycle: ProbeCycle,
    /// Validated program.
    pub plan: ProbePlan,
    /// Stored WCS value at preflight, in millimetres.
    pub wcs_value_mm: Position,
    /// G92 at preflight, in millimetres.
    pub g92_mm: Position,
    /// TLO Z at preflight, in millimetres.
    pub tlo_z_mm: f64,
}

/// One cycle's optional anchor, centre and radius.
type GeometryInput = (Option<Position>, Option<Position>, Option<f64>);

/// The preflight sheet: rows, blockers, and a runnable setup when legal.
#[derive(Clone, Debug)]
pub struct ProbePreflight {
    /// Gate and context rows.
    pub rows: Vec<PreflightRow>,
    /// Human-readable refusal reasons.
    pub blockers: Vec<String>,
    /// Runnable setup, absent when refused or invalid.
    pub setup: Option<ProbeSetup>,
}

/// Lifecycle of one probe run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbePhase {
    /// The program is loaded/started and results are being bound.
    Running,
    /// Every expected result arrived and the value was computed.
    Complete,
    /// A validation, probe, job or alarm failure stopped the run.
    Failed,
}

impl ProbePhase {
    /// Short label for the result panel.
    pub const fn label(self) -> &'static str {
        match self {
            ProbePhase::Running => "PROBING",
            ProbePhase::Complete => "COMPLETE",
            ProbePhase::Failed => "FAILED",
        }
    }
}

/// One run's plan, correlation state and result.
#[derive(Clone, Debug)]
pub struct ProbeRun {
    /// Plan and offset snapshot used by this run.
    pub setup: ProbeSetup,
    /// Raw controller results, in plan order.
    pub results: Vec<ProbeResult>,
    /// Formatted raw `PRB` lines for audit.
    pub raw: Vec<String>,
    /// Computed outcome once the run completed.
    pub outcome: Option<ProbeOutcome>,
    /// Failure text; never auto-resumed.
    pub error: Option<String>,
    /// Run lifecycle.
    pub phase: ProbePhase,
    /// The `JobLoaded` event matched this plan.
    pub loaded: bool,
    /// The `JobStarted` event arrived for this plan.
    pub started: bool,
    /// Extra results bound beyond the plan's expected count.
    pub unexpected_results: u32,
    /// The operator is confirming an Apply.
    pub apply_confirm: bool,
    /// The operator chose Keep.
    pub kept: bool,
    /// A `G10 L2` line was sent by this run.
    pub applied_line: Option<String>,
    /// 1-based source lines at which the runner should pause after each probe.
    ///
    /// The app sets one engine breakpoint on the retract line immediately after
    /// every `G38.2`. That guarantees the PRB event is bound before the next
    /// motion can replace or race the probe, and on the simulator it lets the
    /// probe motion finish before the retract is streamed.
    pub checkpoints: Vec<u32>,
    /// The probe index after which the job is paused, if any.
    pub paused_after_probe: Option<usize>,
}

impl ProbeRun {
    fn new(setup: ProbeSetup) -> Self {
        let checkpoints = probe_checkpoints(&setup.plan.program);
        ProbeRun {
            setup,
            results: Vec::new(),
            raw: Vec::new(),
            outcome: None,
            error: None,
            phase: ProbePhase::Running,
            loaded: false,
            started: false,
            unexpected_results: 0,
            apply_confirm: false,
            kept: false,
            applied_line: None,
            checkpoints,
            paused_after_probe: None,
        }
    }

    /// Number of results bound so far.
    #[must_use]
    pub fn bound(&self) -> usize {
        self.results.len()
    }

    /// Expected result count.
    #[must_use]
    pub fn expected(&self) -> usize {
        self.setup.plan.expected_probes
    }
}

/// The probe workspace model.
#[derive(Clone, Debug)]
pub struct ProbeModel {
    card: Option<ProbeCard>,
    fields: [String; 17],
    /// Explicit `probe connected` confirmation for controllers without `PC`.
    pub probe_connected_confirmed: bool,
    /// X edge approach direction.
    pub edge_x_approach: Sign,
    /// Y edge approach direction.
    pub edge_y_approach: Sign,
    /// X edge is on the outside or inside of the feature.
    pub edge_x_mode: EdgeMode,
    /// Y edge is on the outside or inside of the feature.
    pub edge_y_mode: EdgeMode,
    /// Circle is a bore (`Inside`) or a boss (`Outside`).
    pub circle_mode: EdgeMode,
    /// Circle touch count, `3..=8`.
    pub circle_touches: u8,
    /// Rotation approach along Y for a nominally X-aligned edge.
    pub rotation_approach: Sign,
    run: Option<ProbeRun>,
    /// Name of the most recent probe program loaded into the job runner.
    last_probe_program: Option<String>,
}

impl Default for ProbeModel {
    fn default() -> Self {
        let fields = std::array::from_fn(|_| String::new());
        let mut model = ProbeModel {
            card: None,
            fields,
            probe_connected_confirmed: false,
            edge_x_approach: Sign::Positive,
            edge_y_approach: Sign::Positive,
            edge_x_mode: EdgeMode::Outside,
            edge_y_mode: EdgeMode::Outside,
            circle_mode: EdgeMode::Inside,
            circle_touches: 4,
            rotation_approach: Sign::Negative,
            run: None,
            last_probe_program: None,
        };
        for field in ProbeField::ALL {
            if let Some(slot) = model.fields.get_mut(field.index()) {
                *slot = field.default_text();
            }
        }
        model
    }
}

impl ProbeModel {
    /// Selected card, or `None` for the card grid.
    #[must_use]
    pub fn card(&self) -> Option<ProbeCard> {
        self.card
    }

    /// Current run, if one was started.
    #[must_use]
    pub fn run(&self) -> Option<&ProbeRun> {
        self.run.as_ref()
    }

    /// Mutable run, if one was started.
    pub fn run_mut(&mut self) -> Option<&mut ProbeRun> {
        self.run.as_mut()
    }

    /// A run is active and must not be replaced or navigated away from.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| run.phase == ProbePhase::Running)
    }

    /// Whether the currently loaded job is a generated probe program.
    #[must_use]
    pub fn owns_program(&self, name: &str) -> bool {
        !name.is_empty() && self.last_probe_program.as_deref() == Some(name)
    }

    /// Field text for the editor.
    #[must_use]
    pub fn field_text(&self, field: ProbeField) -> &str {
        self.fields
            .get(field.index())
            .map_or("", std::string::String::as_str)
    }

    /// Validate and store one field from the shared keypad.
    ///
    /// # Errors
    ///
    /// Returns a message for a non-numeric, non-finite or out-of-range value.
    /// The stored text is left unchanged on error.
    pub fn set_field_text(
        &mut self,
        field: ProbeField,
        text: impl Into<String>,
    ) -> Result<(), String> {
        let text = text.into();
        let trimmed = text.trim();
        let value = trimmed
            .parse::<f64>()
            .map_err(|_| format!("{} must be numeric", field.label()))?;
        if !value.is_finite() {
            return Err(format!("{} must be finite", field.label()));
        }
        let (min, max) = field.bounds();
        if value < min || value > max {
            return Err(format!(
                "{} must be within {min:.3} ..= {max:.3} {}",
                field.label(),
                field.unit()
            ));
        }
        if field == ProbeField::Touches && (value.fract() != 0.0) {
            return Err("TOUCHES must be a whole number".to_string());
        }
        if let Some(slot) = self.fields.get_mut(field.index()) {
            *slot = trimmed.to_string();
        }
        Ok(())
    }

    /// Numeric value of a field, in millimetres or the field's unit.
    ///
    /// # Errors
    ///
    /// Returns a message for an empty, non-numeric or non-finite field.
    pub fn field_value(&self, field: ProbeField) -> Result<f64, String> {
        let text = self.field_text(field);
        if text.trim().is_empty() {
            return Err(format!("{} is empty", field.label()));
        }
        let value = text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{} must be numeric", field.label()))?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("{} must be finite", field.label()))
        }
    }

    /// Reset the geometry defaults for the selected card from the live pose.
    ///
    /// Values the operator edits after this are not overwritten by snapshots.
    pub fn reseed(&mut self, ctx: &ProbeContext) {
        let z_mm = ctx.machine_pos_mm.get(CoreAxis::Z);
        let x_mm = ctx.machine_pos_mm.get(CoreAxis::X);
        let y_mm = ctx.machine_pos_mm.get(CoreAxis::Y);
        if !z_mm.is_finite() || !x_mm.is_finite() || !y_mm.is_finite() {
            return;
        }
        let seeds = [
            (ProbeField::SafeZ, z_mm + 10.0),
            (ProbeField::Search, 5.0),
            (ProbeField::Feed, 100.0),
            (ProbeField::Retract, 2.0),
            (ProbeField::Diameter, 3.0),
            (ProbeField::EdgeOffset, 0.0),
            (ProbeField::PlateThickness, 10.0),
            (ProbeField::ExpectedZ, z_mm - 5.0),
            (ProbeField::ProbingHeight, z_mm),
            (ProbeField::ExpectedX, x_mm + 20.0),
            (ProbeField::ExpectedY, y_mm + 20.0),
            (ProbeField::CenterX, x_mm),
            (ProbeField::CenterY, y_mm),
            (ProbeField::Radius, 20.0),
            (ProbeField::ReferenceZ, z_mm - 5.0),
        ];
        for (field, value) in seeds {
            if let Some(slot) = self.fields.get_mut(field.index()) {
                *slot = format!("{value:.3}");
            }
        }
        if let Some(slot) = self.fields.get_mut(ProbeField::Touches.index()) {
            *slot = self.circle_touches.to_string();
        }
        if let Some(slot) = self.fields.get_mut(ProbeField::Separation.index()) {
            *slot = "50.000".to_string();
        }
    }

    /// Open a card and seed its geometry from the current pose.
    ///
    /// Ignored while a probe run is active: a new card must never replace the
    /// plan whose results are still being bound.
    pub fn select_card(&mut self, card: ProbeCard, ctx: &ProbeContext) -> bool {
        if self.is_running() {
            return false;
        }
        self.card = Some(card);
        self.run = None;
        self.reseed(ctx);
        true
    }

    /// Return to the card grid. Ignored while a run is active.
    pub fn close_card(&mut self) -> bool {
        if self.is_running() {
            return false;
        }
        self.card = None;
        self.run = None;
        true
    }

    /// Clear a finished result without changing offsets.
    pub fn keep_result(&mut self) {
        if let Some(run) = self.run.as_mut() {
            run.apply_confirm = false;
            run.kept = true;
        }
    }

    /// Build parameters and a validated plan from the current inputs.
    ///
    /// # Errors
    ///
    /// Returns the first invalid field or the `ghs-probe` validation message.
    pub fn prepare(&self, ctx: &ProbeContext) -> Result<ProbeSetup, String> {
        let card = self
            .card
            .ok_or_else(|| "select a probe cycle".to_string())?;
        if card == ProbeCard::HeightMap {
            return Err("height mapping is deferred in this build".to_string());
        }
        let safe_z_mm = self.field_value(ProbeField::SafeZ)?;
        let search_distance_mm = self.field_value(ProbeField::Search)?;
        let feed_mm_min = self.field_value(ProbeField::Feed)?;
        let retract_mm = self.field_value(ProbeField::Retract)?;
        let probe_diameter_mm = self.field_value(ProbeField::Diameter)?;
        let edge_offset_mm = self.field_value(ProbeField::EdgeOffset)?;
        let (position, center, radius_mm) = self.geometry(card, ctx)?;
        let params = ProbeParams {
            safe_z_mm,
            search_distance_mm,
            feed_mm_min,
            retract_mm,
            probe_diameter_mm,
            edge_offset_mm,
            position,
            center,
            radius_mm,
            wcs_offset_mm: ctx.offset_mm(),
            wcs: ctx.wcs,
            // The probe workspace is millimetre-only and the plan forces G21.
            // `ctx.units` is still shown so an inch DRO is not a surprise.
            units: Units::Mm,
        };
        let cycle = self.cycle(card, ctx)?;
        let mut plan = cycle.plan(&params).map_err(|error| error.to_string())?;
        // `ghs-gcode` currently treats a bare `G91` line while `G38.2` is the
        // modal motion as a probe move with no axis word, so the engine would
        // reject the retract block. Merge `G91` with the relative Z lift: the
        // motion is identical (`G91 G0 Z<retract>`), the parser sees an axis
        // word, and the following `G90` restores absolute mode. Reported to the
        // orchestrator as a parser bug; this is the only plan normalization.
        plan.program = plan.program.replace("\nG91\nG0 Z", "\nG91 G0 Z");
        Ok(ProbeSetup {
            params,
            cycle,
            plan,
            wcs_value_mm: ctx.wcs_value_mm,
            g92_mm: ctx.g92_mm,
            tlo_z_mm: ctx.tlo_z_mm,
        })
    }

    fn geometry(&self, card: ProbeCard, ctx: &ProbeContext) -> Result<GeometryInput, String> {
        let height = self.field_value(ProbeField::ProbingHeight)?;
        let x = ctx.machine_pos_mm.get(CoreAxis::X);
        let y = ctx.machine_pos_mm.get(CoreAxis::Y);
        Ok(match card {
            ProbeCard::ZTouchPlate => (
                Some(Position::xyz(x, y, ctx.machine_pos_mm.get(CoreAxis::Z))),
                None,
                None,
            ),
            ProbeCard::ToolLength => (None, None, None),
            ProbeCard::EdgeX | ProbeCard::EdgeY | ProbeCard::Corner | ProbeCard::Rotation => {
                (Some(Position::xyz(x, y, height)), None, None)
            }
            ProbeCard::CenterCircle => {
                let cx = self.field_value(ProbeField::CenterX)?;
                let cy = self.field_value(ProbeField::CenterY)?;
                let radius = self.field_value(ProbeField::Radius)?;
                (None, Some(Position::xyz(cx, cy, height)), Some(radius))
            }
            ProbeCard::HeightMap => (None, None, None),
        })
    }

    fn cycle(&self, card: ProbeCard, _ctx: &ProbeContext) -> Result<ProbeCycle, String> {
        Ok(match card {
            ProbeCard::ZTouchPlate => ProbeCycle::ZTouchPlate {
                expected_plate_z_mm: self.field_value(ProbeField::ExpectedZ)?,
                plate_thickness_mm: self.field_value(ProbeField::PlateThickness)?,
            },
            ProbeCard::ToolLength => ProbeCycle::ToolLength {
                reference_z_mm: self.field_value(ProbeField::ReferenceZ)?,
            },
            ProbeCard::EdgeX => ProbeCycle::Edge(EdgeSpec {
                axis: ProbeAxis::X,
                mode: self.edge_x_mode,
                approach: self.edge_x_approach,
                expected_edge_mm: self.field_value(ProbeField::ExpectedX)?,
            }),
            ProbeCard::EdgeY => ProbeCycle::Edge(EdgeSpec {
                axis: ProbeAxis::Y,
                mode: self.edge_y_mode,
                approach: self.edge_y_approach,
                expected_edge_mm: self.field_value(ProbeField::ExpectedY)?,
            }),
            ProbeCard::Corner => ProbeCycle::Corner(CornerProbe {
                x: EdgeSpec {
                    axis: ProbeAxis::X,
                    mode: self.edge_x_mode,
                    approach: self.edge_x_approach,
                    expected_edge_mm: self.field_value(ProbeField::ExpectedX)?,
                },
                y: EdgeSpec {
                    axis: ProbeAxis::Y,
                    mode: self.edge_y_mode,
                    approach: self.edge_y_approach,
                    expected_edge_mm: self.field_value(ProbeField::ExpectedY)?,
                },
            }),
            ProbeCard::CenterCircle => {
                let touches = self.field_value(ProbeField::Touches)?;
                let touches = u8::try_from(touches.round() as i64).unwrap_or(0);
                ProbeCycle::CenterCircle {
                    touch_count: touches,
                    mode: self.circle_mode,
                }
            }
            ProbeCard::Rotation => ProbeCycle::Rotation(RotationProbe {
                separation_mm: self.field_value(ProbeField::Separation)?,
                edge_axis: ProbeAxis::X,
                approach: self.rotation_approach,
                expected_edge_mm: self.field_value(ProbeField::ExpectedY)?,
            }),
            ProbeCard::HeightMap => return Err("height mapping is deferred".to_string()),
        })
    }

    /// Build the mandatory preflight sheet.
    ///
    /// The returned `setup` exists only when every gate passes and the plan is
    /// valid; the shell must not start a run otherwise.
    #[must_use]
    pub fn preflight(&self, ctx: &ProbeContext) -> ProbePreflight {
        let mut rows = Vec::new();
        let mut blockers = Vec::new();
        let mut push = |label, value: String, state: RowState| {
            rows.push(PreflightRow {
                label,
                value,
                state,
            });
        };

        push(
            "LINK",
            if ctx.ready {
                "READY".to_string()
            } else if ctx.live {
                "NOT READY".to_string()
            } else {
                "NO LIVE ENGINE".to_string()
            },
            if ctx.ready {
                RowState::Ok
            } else {
                RowState::Bad
            },
        );
        if !ctx.ready {
            blockers.push(if ctx.live {
                "the link is not ready".to_string()
            } else {
                "probing needs a live engine; mock mode has no motion".to_string()
            });
        }

        let idle = ctx.machine_state == MachineState::Idle;
        push(
            "MACHINE",
            ctx.machine_state.as_str().to_string(),
            if idle { RowState::Ok } else { RowState::Bad },
        );
        if !idle {
            blockers.push(format!(
                "the machine must be Idle, not {}",
                ctx.machine_state.as_str()
            ));
        }
        push(
            "JOB",
            if ctx.job_active {
                "ACTIVE".to_string()
            } else {
                "IDLE".to_string()
            },
            if ctx.job_active {
                RowState::Bad
            } else {
                RowState::Ok
            },
        );
        if ctx.job_active {
            blockers.push("a job is already active".to_string());
        }
        push(
            "SPINDLE",
            if ctx.spindle_on { "ON" } else { "OFF" }.to_string(),
            if ctx.spindle_on {
                RowState::Bad
            } else {
                RowState::Ok
            },
        );
        if ctx.spindle_on {
            blockers.push("spindle is on; stop it before probing".to_string());
        }
        push(
            "COOLANT",
            if ctx.coolant_on { "ON" } else { "OFF" }.to_string(),
            if ctx.coolant_on {
                RowState::Bad
            } else {
                RowState::Ok
            },
        );
        if ctx.coolant_on {
            blockers.push("coolant is on; turn it off before probing".to_string());
        }

        if ctx.has_probe {
            push(
                "Pn PROBE",
                if ctx.probe_triggered {
                    "TRIGGERED".to_string()
                } else {
                    "CLEAR".to_string()
                },
                if ctx.probe_triggered {
                    RowState::Bad
                } else {
                    RowState::Ok
                },
            );
            if ctx.probe_triggered {
                blockers.push(
                    "the probe input is already triggered; check the tip/contact wiring"
                        .to_string(),
                );
            }
        } else {
            push("Pn PROBE", "NOT REPORTED".to_string(), RowState::Bad);
            blockers.push("the controller reports no probe input".to_string());
        }

        let verified = if ctx.has_probe_connected_signal {
            ctx.pc_verified()
        } else {
            self.probe_connected_confirmed
        };
        push(
            "PROBE VERIFIED",
            if ctx.has_probe_connected_signal {
                if ctx.pc_verified() {
                    "PC SIGNAL: CONNECTED".to_string()
                } else {
                    "PC SIGNAL: DISCONNECTED".to_string()
                }
            } else if self.probe_connected_confirmed {
                "OPERATOR CHECKED (NO PC)".to_string()
            } else {
                "NOT CONFIRMED".to_string()
            },
            if verified {
                RowState::Ok
            } else {
                RowState::Bad
            },
        );
        if !verified {
            blockers.push(if ctx.has_probe_connected_signal {
                "the PC probe-connected signal reports disconnected".to_string()
            } else {
                "confirm the probe is connected (this controller has no PC signal)".to_string()
            });
        }

        push(
            "OFFSETS",
            if ctx.offsets_reported {
                "$# LOADED".to_string()
            } else {
                "$# MISSING".to_string()
            },
            if ctx.offsets_reported {
                RowState::Ok
            } else {
                RowState::Bad
            },
        );
        if !ctx.offsets_reported {
            blockers.push("wait for `$#`; the WCS/G92 conversion is not known".to_string());
        }

        push("WCS", ctx.wcs.as_str().to_string(), RowState::Info);
        push(
            "WCS OFFSET",
            position_text(ctx.wcs_value_mm),
            RowState::Info,
        );
        push("G92", position_text(ctx.g92_mm), RowState::Info);
        push(
            "TLO Z",
            format!("{:.3} mm (included)", ctx.tlo_z_mm),
            if ctx.tlo_z_mm == 0.0 {
                RowState::Info
            } else {
                RowState::Warn
            },
        );
        push(
            "USED OFFSET",
            format!(
                "{} (machine = work + offset)",
                position_text(ctx.offset_mm())
            ),
            RowState::Info,
        );
        push(
            "MACHINE XYZ",
            format!("{} mm", position_text(ctx.machine_pos_mm)),
            RowState::Info,
        );
        push(
            "WORK XYZ",
            format!("{} mm", position_text(ctx.work_pos_mm)),
            RowState::Info,
        );
        push(
            "UNITS",
            format!("probe fields mm; DRO {}", ctx.units.suffix()),
            RowState::Info,
        );

        let setup = match self.prepare(ctx) {
            Ok(setup) => {
                push(
                    "SAFE Z",
                    format!("{:.3} mm", setup.params.safe_z_mm),
                    RowState::Info,
                );
                push(
                    "SEARCH",
                    format!(
                        "{:.3} mm at {:.0} mm/min",
                        setup.params.search_distance_mm, setup.params.feed_mm_min
                    ),
                    RowState::Info,
                );
                push(
                    "PROBE DIA",
                    format!(
                        "{:.3} mm (tip radius {:.3})",
                        setup.params.probe_diameter_mm,
                        setup.params.probe_radius_mm()
                    ),
                    RowState::Info,
                );
                push(
                    "EDGE OFFSET",
                    format!("{:.3} mm", setup.params.edge_offset_mm),
                    RowState::Info,
                );
                push(
                    "RETRACT",
                    format!("{:.3} mm", setup.params.retract_mm),
                    RowState::Info,
                );
                push(
                    "EXPECTED",
                    expected_text(&setup.cycle, &setup.params),
                    RowState::Info,
                );
                push(
                    "TRAVEL",
                    format!(
                        "{} probes, {:.3} mm total search",
                        setup.plan.expected_probes,
                        setup.plan.total_search_travel_mm()
                    ),
                    RowState::Info,
                );
                push(
                    "PROGRAM",
                    format!(
                        "{} lines, {}",
                        setup.plan.program.lines().count(),
                        setup.plan.name
                    ),
                    RowState::Info,
                );
                push(
                    "SAFETY",
                    "G21 G90 G17 G94, G38.2, M5, M9, safe-Z retract".to_string(),
                    RowState::Ok,
                );
                Some(setup)
            }
            Err(error) => {
                push("PLAN", error.clone(), RowState::Bad);
                blockers.push(error.clone());
                None
            }
        };

        ProbePreflight {
            rows,
            setup: if blockers.is_empty() { setup } else { None },
            blockers,
        }
    }

    /// Start a run from a validated preflight setup.
    ///
    /// The previous result is discarded; offsets are untouched.
    pub fn begin(&mut self, setup: ProbeSetup) {
        self.last_probe_program = Some(setup.plan.name.clone());
        self.run = Some(ProbeRun::new(setup));
    }

    /// Bind a `JobLoaded` event.
    pub fn on_job_loaded(&mut self, name: &str) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase == ProbePhase::Running && name == run.setup.plan.name {
            run.loaded = true;
            true
        } else {
            false
        }
    }

    /// Bind a `JobStarted` event.
    pub fn on_job_started(&mut self) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase == ProbePhase::Running && run.loaded {
            run.started = true;
            true
        } else {
            false
        }
    }

    /// Bind a `JobPaused` event.
    ///
    /// Only a breakpoint at one of this run's probe checkpoints is recorded;
    /// an operator feed hold or another pause never triggers an automatic
    /// resume.
    pub fn on_job_paused(&mut self, line: u32, breakpoint: bool) -> bool {
        if !breakpoint {
            return false;
        }
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase != ProbePhase::Running || !run.started {
            return false;
        }
        let index = run
            .checkpoints
            .iter()
            .position(|checkpoint| *checkpoint == line);
        run.paused_after_probe = index;
        index.is_some()
    }

    /// Whether the job is paused at a probe checkpoint whose PRB is bound and
    /// may be resumed.
    #[must_use]
    pub fn needs_resume(&self) -> bool {
        self.run.as_ref().is_some_and(|run| {
            run.phase == ProbePhase::Running
                && run
                    .paused_after_probe
                    .is_some_and(|index| run.results.len() > index)
        })
    }

    /// Whether the job is paused at a probe checkpoint still waiting for PRB.
    #[must_use]
    pub fn waiting_for_probe(&self) -> bool {
        self.run.as_ref().is_some_and(|run| {
            run.phase == ProbePhase::Running
                && run
                    .paused_after_probe
                    .is_some_and(|index| run.results.len() <= index)
        })
    }

    /// Record that the shell sent `ResumeProgram` for a probe checkpoint.
    pub fn mark_resumed(&mut self) {
        if let Some(run) = self.run.as_mut() {
            run.paused_after_probe = None;
        }
    }

    /// Bind the next `PRB` result, in plan order.
    ///
    /// Results are accepted only after the matching `JobLoaded` and
    /// `JobStarted`; an unrelated probe (MDI, other job) is never attributed
    /// to this cycle.
    pub fn bind_result(&mut self, result: ProbeResult) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase != ProbePhase::Running || !run.started {
            return false;
        }
        run.raw.push(result.to_string());
        if run.results.len() < run.setup.plan.expected_probes {
            run.results.push(result);
        } else {
            run.unexpected_results = run.unexpected_results.saturating_add(1);
        }
        true
    }

    /// Finish a run whose job completed and compute the outcome.
    pub fn on_job_completed(&mut self) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase != ProbePhase::Running {
            return false;
        }
        if !run.started {
            run.phase = ProbePhase::Failed;
            run.error = Some("the job ended before the probe program started".to_string());
            return true;
        }
        if run.unexpected_results > 0 {
            run.phase = ProbePhase::Failed;
            run.error = Some(format!(
                "{} unexpected probe result(s); offsets were not computed",
                run.unexpected_results
            ));
            return true;
        }
        match ProbeOutcome::from_results(&run.setup.cycle, &run.setup.params, &run.results) {
            Ok(outcome) => {
                run.outcome = Some(outcome);
                run.phase = ProbePhase::Complete;
            }
            Err(error) => {
                run.phase = ProbePhase::Failed;
                run.error = Some(error.to_string());
            }
        }
        true
    }

    /// Stop a run after a job error, abort, alarm, disconnect or watchdog.
    ///
    /// Only a running probe is failed; a value that already completed stays
    /// visible for audit, while Apply is gated by the live machine state.
    /// Keeps the partial raw results and never resumes.
    pub fn fail(&mut self, reason: impl Into<String>) -> bool {
        let reason = reason.into();
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase != ProbePhase::Running {
            return false;
        }
        run.phase = ProbePhase::Failed;
        run.error = Some(reason);
        run.apply_confirm = false;
        true
    }

    /// A `JobAborted` event. The reason is already user-facing.
    pub fn on_job_aborted(&mut self, reason: &str) -> bool {
        self.fail(format!("job stopped: {reason}"))
    }

    /// A `JobError` event.
    pub fn on_job_error(&mut self, line: u32, message: &str) -> bool {
        self.fail(format!("controller rejected line {line}: {message}"))
    }

    /// An alarm event.
    pub fn on_alarm(&mut self, code: u16) -> bool {
        self.fail(format!(
            "ALARM:{code}; recover manually before another probe"
        ))
    }

    /// A disconnect event.
    pub fn on_disconnected(&mut self) -> bool {
        self.fail("disconnected; telemetry and offsets are stale".to_string())
    }

    /// Offset targets implied by the completed outcome.
    ///
    /// Returns the values that will be written to `G10 L2` after subtracting
    /// `G92` and `TLO`, keyed by axis.
    ///
    /// # Errors
    ///
    /// Returns a message when the run is incomplete, or when the cycle's
    /// application is deliberately deferred.
    pub fn apply_values(&self) -> Result<Vec<(CoreAxis, f64)>, String> {
        let run = self
            .run
            .as_ref()
            .ok_or_else(|| "no probe run to apply".to_string())?;
        if run.phase != ProbePhase::Complete {
            return Err("the probe run has not completed".to_string());
        }
        let outcome = run
            .outcome
            .as_ref()
            .ok_or_else(|| "no computed probe value".to_string())?;
        let targets = apply_targets(&outcome.value)?;
        let mut values = Vec::with_capacity(targets.len());
        for (axis, target_mm) in targets {
            let mut wcs_value = target_mm - run.setup.g92_mm.get(axis);
            if axis == CoreAxis::Z {
                wcs_value -= run.setup.tlo_z_mm;
            }
            if !wcs_value.is_finite() {
                return Err("computed WCS value is not finite".to_string());
            }
            values.push((axis, wcs_value));
        }
        Ok(values)
    }

    /// The `G10 L2` line for a completed outcome.
    ///
    /// # Errors
    ///
    /// See [`ProbeModel::apply_values`].
    pub fn apply_lines(&self) -> Result<Vec<String>, String> {
        let values = self.apply_values()?;
        let Some(run) = self.run.as_ref() else {
            return Err("no probe run to apply".to_string());
        };
        let mut words = String::new();
        for (axis, value) in values {
            words.push(' ');
            words.push_str(axis.as_str());
            words.push_str(&format_mm(value));
        }
        let wcs_number = run.setup.params.wcs.index() + 1;
        Ok(vec![format!("G10 L2 P{wcs_number}{words}")])
    }

    /// Whether the completed cycle supports offset application.
    #[must_use]
    pub fn apply_supported(&self) -> bool {
        self.run
            .as_ref()
            .and_then(|run| run.outcome.as_ref())
            .is_some_and(|outcome| apply_targets(&outcome.value).is_ok())
    }

    /// Record that the shell sent the apply line and start the `$#` refresh.
    pub fn mark_applied(&mut self, line: String) {
        if let Some(run) = self.run.as_mut() {
            run.applied_line = Some(line);
            run.apply_confirm = false;
            run.kept = false;
        }
    }
}

/// Offset targets for a computed value: `(axis, machine coordinate)`.
fn apply_targets(value: &ProbeValue) -> Result<Vec<(CoreAxis, f64)>, String> {
    Ok(match value {
        ProbeValue::ZZero { zero_z_mm, .. } => vec![(CoreAxis::Z, *zero_z_mm)],
        ProbeValue::Edge { axis, zero_mm, .. } => vec![(axis.axis(), *zero_mm)],
        ProbeValue::Corner { zero, .. } | ProbeValue::XyZero { zero, .. } => {
            vec![(CoreAxis::X, zero.x_mm), (CoreAxis::Y, zero.y_mm)]
        }
        ProbeValue::Center { center, .. } | ProbeValue::RectangleCenter { center, .. } => {
            vec![(CoreAxis::X, center.x_mm), (CoreAxis::Y, center.y_mm)]
        }
        ProbeValue::ToolLength { .. } => {
            return Err(
                "tool-length apply is deferred; no TLO/`$TLR` write is sent from this card"
                    .to_string(),
            );
        }
        ProbeValue::Rotation { .. } => {
            return Err(
                "rotation apply is deferred; the measured angle is reported only".to_string(),
            );
        }
    })
}

/// Headline and detail rows for a completed outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct OutcomeSummary {
    /// Large-print measured value.
    pub headline: String,
    /// Supporting values.
    pub details: Vec<(String, String)>,
}

/// Format one computed outcome for the result sheet.
#[must_use]
pub fn outcome_summary(outcome: &ProbeOutcome) -> OutcomeSummary {
    match &outcome.value {
        ProbeValue::ZZero {
            contact_z_mm,
            plate_thickness_mm,
            zero_z_mm,
        } => OutcomeSummary {
            headline: format!("WORK Z ZERO {}", format_mm(*zero_z_mm)),
            details: vec![
                (
                    "Contact Z".to_string(),
                    format!("{} mm", format_mm(*contact_z_mm)),
                ),
                (
                    "Plate thickness".to_string(),
                    format!("{} mm", format_mm(*plate_thickness_mm)),
                ),
            ],
        },
        ProbeValue::ToolLength {
            reference_z_mm,
            contact_z_mm,
            offset_mm,
        } => OutcomeSummary {
            headline: format!("TOOL LENGTH DELTA {}", format_mm(*offset_mm)),
            details: vec![
                (
                    "Reference Z".to_string(),
                    format!("{} mm", format_mm(*reference_z_mm)),
                ),
                (
                    "Contact Z".to_string(),
                    format!("{} mm", format_mm(*contact_z_mm)),
                ),
                ("Apply".to_string(), "TLO apply is deferred".to_string()),
            ],
        },
        ProbeValue::Edge {
            axis,
            edge_mm,
            zero_mm,
            ..
        } => OutcomeSummary {
            headline: format!("WORK {} ZERO {}", axis.as_str(), format_mm(*zero_mm)),
            details: vec![(
                "Measured edge".to_string(),
                format!("{} mm", format_mm(*edge_mm)),
            )],
        },
        ProbeValue::Corner {
            x_edge_mm,
            y_edge_mm,
            zero,
        } => OutcomeSummary {
            headline: format!(
                "WORK X0 Y0 {} {}",
                format_mm(zero.x_mm),
                format_mm(zero.y_mm)
            ),
            details: vec![
                (
                    "Measured X edge".to_string(),
                    format!("{} mm", format_mm(*x_edge_mm)),
                ),
                (
                    "Measured Y edge".to_string(),
                    format!("{} mm", format_mm(*y_edge_mm)),
                ),
            ],
        },
        ProbeValue::Center {
            center,
            contact_radius_mm,
            feature_radius_mm,
            rms_error_mm,
        } => OutcomeSummary {
            headline: format!(
                "CENTER {} {}",
                format_mm(center.x_mm),
                format_mm(center.y_mm)
            ),
            details: vec![
                (
                    "Feature radius".to_string(),
                    format!("{} mm", format_mm(*feature_radius_mm)),
                ),
                (
                    "Contact radius".to_string(),
                    format!("{} mm", format_mm(*contact_radius_mm)),
                ),
                (
                    "RMS fit error".to_string(),
                    format!("{} mm", format_mm(*rms_error_mm)),
                ),
            ],
        },
        ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } => OutcomeSummary {
            headline: format!(
                "CENTER {} {}",
                format_mm(center.x_mm),
                format_mm(center.y_mm)
            ),
            details: vec![
                (
                    "Measured width".to_string(),
                    format!("{} mm", format_mm(*width_mm)),
                ),
                (
                    "Measured height".to_string(),
                    format!("{} mm", format_mm(*height_mm)),
                ),
            ],
        },
        ProbeValue::Rotation {
            angle_deg,
            nominal_deg,
            deviation_deg,
            ..
        } => OutcomeSummary {
            headline: format!("ROTATION {} deg", format_mm(*angle_deg)),
            details: vec![
                (
                    "Nominal".to_string(),
                    format!("{} deg", format_mm(*nominal_deg)),
                ),
                (
                    "Deviation".to_string(),
                    format!("{} deg", format_mm(*deviation_deg)),
                ),
                (
                    "Apply".to_string(),
                    "rotation apply is deferred".to_string(),
                ),
            ],
        },
        ProbeValue::XyZero {
            x_edge_mm,
            y_edge_mm,
            zero,
        } => OutcomeSummary {
            headline: format!(
                "WORK X0 Y0 {} {}",
                format_mm(zero.x_mm),
                format_mm(zero.y_mm)
            ),
            details: vec![
                (
                    "Measured X edge".to_string(),
                    format!("{} mm", format_mm(*x_edge_mm)),
                ),
                (
                    "Measured Y edge".to_string(),
                    format!("{} mm", format_mm(*y_edge_mm)),
                ),
            ],
        },
    }
}

/// One-line expected geometry for the preflight.
fn expected_text(cycle: &ProbeCycle, params: &ProbeParams) -> String {
    match cycle {
        ProbeCycle::ZTouchPlate {
            expected_plate_z_mm,
            plate_thickness_mm,
        } => format!(
            "plate top Z {} mm, thickness {} mm",
            format_mm(*expected_plate_z_mm),
            format_mm(*plate_thickness_mm)
        ),
        ProbeCycle::ToolLength { reference_z_mm } => {
            format!("reference surface Z {} mm", format_mm(*reference_z_mm))
        }
        ProbeCycle::Edge(spec) => format!(
            "{} edge {} mm, {} side, {} approach",
            spec.axis.as_str(),
            format_mm(spec.expected_edge_mm),
            spec.mode.as_str(),
            spec.approach.as_str()
        ),
        ProbeCycle::Corner(corner) => format!(
            "X edge {} mm ({}), Y edge {} mm ({})",
            format_mm(corner.x.expected_edge_mm),
            corner.x.approach.as_str(),
            format_mm(corner.y.expected_edge_mm),
            corner.y.approach.as_str()
        ),
        ProbeCycle::CenterCircle { touch_count, mode } => {
            let (cx, cy) = params
                .center
                .map(|center| (center.get(CoreAxis::X), center.get(CoreAxis::Y)))
                .unwrap_or((0.0, 0.0));
            let radius = params.radius_mm.unwrap_or(0.0);
            format!(
                "center X{} Y{}, radius {} mm, {} touches, {}",
                format_mm(cx),
                format_mm(cy),
                format_mm(radius),
                touch_count,
                mode.as_str()
            )
        }
        ProbeCycle::CenterRectangle {
            width_mm,
            height_mm,
            ..
        } => format!(
            "rectangle {} x {} mm",
            format_mm(*width_mm),
            format_mm(*height_mm)
        ),
        ProbeCycle::Rotation(rotation) => format!(
            "edge {} mm, separation {} mm, {} approach",
            format_mm(rotation.expected_edge_mm),
            format_mm(rotation.separation_mm),
            rotation.approach.as_str()
        ),
        ProbeCycle::XyTouchPlate(corner) => format!(
            "plate X {} mm, Y {} mm",
            format_mm(corner.x.expected_edge_mm),
            format_mm(corner.y.expected_edge_mm)
        ),
    }
}

/// 1-based line after every `G38.2` in a generated program.
///
/// The app pauses there, binds the matching PRB, then resumes. A program with
/// no probe lines yields an empty list.
#[must_use]
pub fn probe_checkpoints(program: &str) -> Vec<u32> {
    let mut checkpoints = Vec::new();
    for (index, line) in program.lines().enumerate() {
        if line.trim_start().starts_with("G38.2")
            && let Ok(next) = u32::try_from(index + 2)
        {
            checkpoints.push(next);
        }
    }
    checkpoints
}

/// Format a millimetre value with three decimals, normalising negative zero.
pub fn format_mm(value: f64) -> String {
    let rounded = (value * 1_000.0).round() / 1_000.0;
    if rounded == 0.0 || rounded.abs() < 0.000_5 {
        "0.000".to_string()
    } else {
        format!("{rounded:.3}")
    }
}

/// `X.. Y.. Z..` text for a position, in millimetres.
pub fn position_text(position: Position) -> String {
    Position::xyz(
        position.get(CoreAxis::X),
        position.get(CoreAxis::Y),
        position.get(CoreAxis::Z),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ProbeContext {
        ProbeContext {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::xyz(10.0, 20.0, -1.0),
            work_pos_mm: Position::xyz(1.0, 2.0, -3.0),
            wcs: Wcs::G54,
            wcs_value_mm: Position::xyz(5.0, 6.0, 7.0),
            g92_mm: Position::xyz(1.0, 2.0, 3.0),
            tlo_z_mm: 4.0,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: DisplayUnits::Millimetre,
        }
    }

    fn model(card: ProbeCard) -> ProbeModel {
        let mut model = ProbeModel::default();
        let mut ctx = context();
        ctx.machine_pos_mm = Position::xyz(0.0, 0.0, 0.0);
        assert!(model.select_card(card, &ctx));
        model.probe_connected_confirmed = true;
        model
    }

    #[test]
    fn preflight_refuses_spindle_coolant_and_unverified_probe() {
        let mut ctx = context();
        ctx.spindle_on = true;
        ctx.coolant_on = true;
        let model = model(ProbeCard::ZTouchPlate);
        let sheet = model.preflight(&ctx);
        assert!(sheet.setup.is_none());
        assert!(
            sheet
                .blockers
                .iter()
                .any(|reason| reason.contains("spindle"))
        );
        assert!(
            sheet
                .blockers
                .iter()
                .any(|reason| reason.contains("coolant"))
        );

        let mut unverified = model;
        unverified.probe_connected_confirmed = false;
        let sheet = unverified.preflight(&context());
        assert!(
            sheet
                .blockers
                .iter()
                .any(|reason| reason.contains("PC") || reason.contains("confirm the probe"))
        );
    }

    #[test]
    fn preflight_uses_the_pc_signal_when_reported() {
        let mut ctx = context();
        ctx.has_probe_connected_signal = true;
        ctx.probe_disconnected = true;
        let model = model(ProbeCard::ZTouchPlate);
        let sheet = model.preflight(&ctx);
        assert!(sheet.setup.is_none());
        assert!(
            sheet
                .blockers
                .iter()
                .any(|reason| reason.contains("PC probe-connected"))
        );

        let mut connected = context();
        connected.has_probe_connected_signal = true;
        let sheet = model.preflight(&connected);
        assert!(sheet.setup.is_some());
    }

    #[test]
    fn wcs_offset_includes_g92_and_tlo_and_the_plan_subtracts_it() {
        let ctx = context();
        assert_eq!(
            ctx.offset_mm(),
            Position::xyz(5.0 + 1.0, 6.0 + 2.0, 7.0 + 3.0 + 4.0)
        );
        let model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("plan");
        // Start Z = expected (-5) + search (5) = 0 machine; offset Z is 14,
        // so the plan emits work Z -14 for the search start.
        assert!(setup.plan.program.contains("G0 Z-14.000"));
        assert_eq!(setup.params.wcs_offset_mm, Position::xyz(6.0, 8.0, 14.0));
        assert_eq!(
            setup.params.to_machine(Position::xyz(0.0, 0.0, -14.0)),
            Position::xyz(6.0, 8.0, 0.0)
        );
    }

    #[test]
    fn completing_a_z_probe_binds_results_in_order() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(!model.on_job_started(), "not started before JobLoaded");
        assert!(model.on_job_loaded("z-touch-plate"));
        assert!(model.on_job_started());
        // A stale result before start would be ignored; this one is after.
        let contact = Position::xyz(6.0, 8.0, -11.0);
        assert!(model.bind_result(ProbeResult::new(true, contact)));
        assert!(model.on_job_completed());
        let run = model.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Complete);
        assert_eq!(run.results.len(), 1);
        let ProbeValue::ZZero { zero_z_mm, .. } = &run.outcome.as_ref().expect("outcome").value
        else {
            panic!("wrong value");
        };
        assert!((*zero_z_mm - (-21.0)).abs() < 1.0e-9);
    }

    #[test]
    fn a_missed_or_alarmed_run_fails_and_never_resumes() {
        let ctx = context();
        let mut model = model(ProbeCard::EdgeX);
        model
            .set_field_text(ProbeField::ExpectedX, "30.000")
            .expect("field");
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("edge-x-outside-positive"));
        assert!(model.on_job_started());
        assert!(model.on_alarm(5));
        let run = model.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Failed);
        assert!(
            run.error
                .as_deref()
                .is_some_and(|text| text.contains("ALARM:5"))
        );
        assert!(!model.bind_result(ProbeResult::new(true, Position::xyz(0.0, 0.0, 0.0))));
    }

    #[test]
    fn circle_results_bind_by_index_and_fit_the_center() {
        let ctx = context();
        let mut model = model(ProbeCard::CenterCircle);
        model
            .set_field_text(ProbeField::Radius, "10.000")
            .expect("radius");
        let setup = model.prepare(&ctx).expect("plan");
        assert_eq!(setup.plan.expected_probes, 4);
        model.begin(setup);
        assert!(model.on_job_loaded("center-circle-inside"));
        assert!(model.on_job_started());
        for (x, y) in [(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0), (0.0, -10.0)] {
            assert!(model.bind_result(ProbeResult::new(true, Position::xyz(x, y, 0.0))));
        }
        assert!(model.on_job_completed());
        let ProbeValue::Center {
            center,
            feature_radius_mm,
            ..
        } = &model
            .run()
            .expect("run")
            .outcome
            .as_ref()
            .expect("outcome")
            .value
        else {
            panic!("wrong value");
        };
        assert!(center.x_mm.abs() < 1.0e-9);
        assert!(center.y_mm.abs() < 1.0e-9);
        assert!((*feature_radius_mm - 11.5).abs() < 1.0e-6);
    }

    #[test]
    fn g10_l2_apply_de_biases_g92_and_tlo() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("z-touch-plate"));
        assert!(model.on_job_started());
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(6.0, 8.0, -11.0))));
        assert!(model.on_job_completed());
        let lines = model.apply_lines().expect("apply");
        // zero = -11 - 10 = -21; minus G92 Z 3 and TLO 4.
        assert_eq!(lines, vec!["G10 L2 P1 Z-28.000".to_string()]);
    }

    #[test]
    fn corner_apply_writes_both_axes_in_one_line() {
        let ctx = context();
        let mut model = model(ProbeCard::Corner);
        model
            .set_field_text(ProbeField::ExpectedX, "30.000")
            .expect("x");
        model
            .set_field_text(ProbeField::ExpectedY, "40.000")
            .expect("y");
        model
            .set_field_text(ProbeField::EdgeOffset, "1.000")
            .expect("offset");
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("corner"));
        assert!(model.on_job_started());
        // +X/+Y outside, radius 1.5: edge = contact + 1.5; zero = edge + 1.
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(27.0, 20.0, -1.0))));
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(10.0, 36.0, -1.0))));
        assert!(model.on_job_completed());
        let lines = model.apply_lines().expect("apply");
        // X = 27 + 1.5 + 1 - G92 1 = 28.5; Y = 36 + 1.5 + 1 - G92 2 = 36.5.
        assert_eq!(lines, vec!["G10 L2 P1 X28.500 Y36.500".to_string()]);
    }

    #[test]
    fn deferred_cycles_do_not_produce_apply_commands() {
        let ctx = context();
        let mut rotation = model(ProbeCard::Rotation);
        rotation
            .set_field_text(ProbeField::ExpectedY, "20.000")
            .expect("edge");
        let setup = rotation.prepare(&ctx).expect("plan");
        rotation.begin(setup);
        assert!(rotation.on_job_loaded("rotation"));
        assert!(rotation.on_job_started());
        assert!(rotation.bind_result(ProbeResult::new(true, Position::xyz(5.0, 20.0, -1.0))));
        assert!(rotation.bind_result(ProbeResult::new(true, Position::xyz(55.0, 20.0, -1.0))));
        assert!(rotation.on_job_completed());
        assert!(!rotation.apply_supported());
        assert!(rotation.apply_lines().is_err());

        let height = model(ProbeCard::HeightMap);
        assert!(height.prepare(&ctx).is_err());
        assert!(ProbeCard::HeightMap.deferred_note().is_some());
    }

    #[test]
    fn invalid_fields_are_rejected_before_any_plan() {
        let ctx = context();
        let mut model = model(ProbeCard::EdgeX);
        assert!(model.set_field_text(ProbeField::Search, "0").is_ok());
        assert!(model.prepare(&ctx).is_err());
        assert!(model.set_field_text(ProbeField::Feed, "5000").is_err());
        assert!(model.set_field_text(ProbeField::Touches, "2").is_err());
    }

    #[test]
    fn probe_checkpoints_pause_after_each_search_and_wait_for_prb() {
        let ctx = context();
        let mut model = model(ProbeCard::Corner);
        let setup = model.prepare(&ctx).expect("plan");
        let checkpoints = probe_checkpoints(&setup.plan.program);
        assert_eq!(checkpoints.len(), 2);
        model.begin(setup);
        assert!(model.on_job_loaded("corner"));
        assert!(model.on_job_started());
        let first = model.run().expect("run").checkpoints[0];
        assert!(model.on_job_paused(first, true));
        assert!(model.waiting_for_probe());
        assert!(!model.needs_resume());
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(27.0, 20.0, -1.0))));
        assert!(model.needs_resume());
        model.mark_resumed();
        assert!(!model.needs_resume());
        let second = model.run().expect("run").checkpoints[1];
        assert!(model.on_job_paused(second, true));
        assert!(model.waiting_for_probe());
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(10.0, 36.0, -1.0))));
        assert!(model.needs_resume());
        model.mark_resumed();
        assert!(model.on_job_completed());
        assert_eq!(model.run().expect("run").phase, ProbePhase::Complete);
    }

    #[test]
    fn preflight_shows_every_mandatory_field() {
        let ctx = context();
        let model = model(ProbeCard::ZTouchPlate);
        let sheet = model.preflight(&ctx);
        let labels: Vec<&str> = sheet.rows.iter().map(|row| row.label).collect();
        for required in [
            "LINK",
            "MACHINE",
            "JOB",
            "SPINDLE",
            "COOLANT",
            "Pn PROBE",
            "PROBE VERIFIED",
            "OFFSETS",
            "WCS",
            "WCS OFFSET",
            "G92",
            "TLO Z",
            "USED OFFSET",
            "MACHINE XYZ",
            "WORK XYZ",
            "SAFE Z",
            "SEARCH",
            "PROBE DIA",
            "EDGE OFFSET",
            "RETRACT",
            "EXPECTED",
            "TRAVEL",
            "PROGRAM",
            "SAFETY",
        ] {
            assert!(
                labels.contains(&required),
                "missing preflight row {required}"
            );
        }
        let expected = sheet
            .rows
            .iter()
            .find(|row| row.label == "EXPECTED")
            .map(|row| row.value.clone())
            .unwrap_or_default();
        assert!(expected.contains("plate top"));
        assert!(sheet.setup.is_some());
    }

    #[test]
    fn every_generated_plan_parses_in_the_engine_parser() {
        let ctx = context();
        for card in [
            ProbeCard::ZTouchPlate,
            ProbeCard::EdgeX,
            ProbeCard::EdgeY,
            ProbeCard::Corner,
            ProbeCard::CenterCircle,
            ProbeCard::Rotation,
            ProbeCard::ToolLength,
        ] {
            let model = model(card);
            let setup = model
                .prepare(&ctx)
                .unwrap_or_else(|error| panic!("{card:?} prepare: {error}"));
            if let Err(errors) = ghs_gcode::parse_program(&setup.plan.program) {
                panic!(
                    "{card:?} plan does not parse: {errors:?}\n{}",
                    setup.plan.program
                );
            }
        }
    }
}
