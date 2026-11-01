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

use std::path::PathBuf;

use ghs_core::Units;
use ghs_engine::{Axis as CoreAxis, MachineState, Position, ProbeResult, Wcs};
use ghs_probe::{
    CornerProbe, EdgeMode, EdgeSpec, HeightMap, HeightMapEstimate, HeightMapGrid,
    MAX_CIRCLE_TOUCHES, MAX_COORDINATE_MM, MAX_EDGE_OFFSET_MM, MAX_HEIGHT_MAP_AXIS_POINTS,
    MAX_HEIGHT_MAP_POINTS, MAX_PLATE_THICKNESS_MM, MAX_PROBE_DIAMETER_MM, MAX_PROBE_FEED_MM_MIN,
    MAX_RADIUS_MM, MAX_RETRACT_MM, MAX_SEARCH_DISTANCE_MM, MAX_SEPARATION_MM, MIN_CIRCLE_TOUCHES,
    ProbeAxis, ProbeCycle, ProbeOutcome, ProbeParams, ProbePlan, ProbeValue, RotationProbe, Sign,
    XyPoint,
};
use ghs_viewer::Point2;

use crate::state::Units as DisplayUnits;
use crate::viewer::{HeightCell, HeightOverlay};

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
    /// Height-map grid origin X, in **work** millimetres.
    OriginX,
    /// Height-map grid origin Y, in **work** millimetres.
    OriginY,
    /// Height-map column spacing, in millimetres.
    StepX,
    /// Height-map row spacing, in millimetres.
    StepY,
    /// Height-map column count.
    Columns,
    /// Height-map row count.
    Rows,
    /// Lowest expected surface Z, in machine millimetres.
    SurfaceZ,
}

impl ProbeField {
    /// Number of editable fields.
    pub const COUNT: usize = 24;

    /// Every field, in edit order.
    pub const ALL: [ProbeField; ProbeField::COUNT] = [
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
        ProbeField::OriginX,
        ProbeField::OriginY,
        ProbeField::StepX,
        ProbeField::StepY,
        ProbeField::Columns,
        ProbeField::Rows,
        ProbeField::SurfaceZ,
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
            ProbeField::OriginX => 17,
            ProbeField::OriginY => 18,
            ProbeField::StepX => 19,
            ProbeField::StepY => 20,
            ProbeField::Columns => 21,
            ProbeField::Rows => 22,
            ProbeField::SurfaceZ => 23,
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
            ProbeField::OriginX => "ORIGIN X WORK",
            ProbeField::OriginY => "ORIGIN Y WORK",
            ProbeField::StepX => "STEP X",
            ProbeField::StepY => "STEP Y",
            ProbeField::Columns => "COLUMNS",
            ProbeField::Rows => "ROWS",
            ProbeField::SurfaceZ => "SURFACE Z MACH",
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
            ProbeField::OriginX => "origin-x",
            ProbeField::OriginY => "origin-y",
            ProbeField::StepX => "step-x",
            ProbeField::StepY => "step-y",
            ProbeField::Columns => "columns",
            ProbeField::Rows => "rows",
            ProbeField::SurfaceZ => "surface-z",
        }
    }

    /// Unit suffix shown in the keypad hint.
    pub const fn unit(self) -> &'static str {
        match self {
            ProbeField::Touches
            | ProbeField::Columns
            | ProbeField::Rows
            | ProbeField::StepX
            | ProbeField::StepY => "",
            ProbeField::Feed => "mm/min",
            _ => "mm",
        }
    }

    /// Whether the field only accepts a whole number.
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            ProbeField::Touches | ProbeField::Columns | ProbeField::Rows
        )
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
            ProbeField::StepX | ProbeField::StepY => (0.0, MAX_COORDINATE_MM),
            ProbeField::Columns | ProbeField::Rows => (2.0, MAX_HEIGHT_MAP_AXIS_POINTS as f64),
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
            ProbeField::OriginX => "0.000".to_string(),
            ProbeField::OriginY => "0.000".to_string(),
            ProbeField::StepX => "10.000".to_string(),
            ProbeField::StepY => "10.000".to_string(),
            ProbeField::Columns => "4".to_string(),
            ProbeField::Rows => "4".to_string(),
            ProbeField::SurfaceZ => "5.000".to_string(),
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
            ProbeCard::HeightMap => "Bounded Z grid; measured map overlay, no job compensation.",
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
                "Row-major grid of Z touches with a retract between points. Missing cells stay \
                 invalid; this build measures and exports the map but does not compensate the job."
            }
        }
    }

    /// Whether this card has an explicitly deferred apply step.
    pub const fn deferred_note(self) -> Option<&'static str> {
        match self {
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
            ProbeCard::HeightMap => &[
                ProbeField::OriginX,
                ProbeField::OriginY,
                ProbeField::StepX,
                ProbeField::StepY,
                ProbeField::Columns,
                ProbeField::Rows,
                ProbeField::SurfaceZ,
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
                ProbeField::Diameter,
            ],
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

/// What a probe plan measures.
#[derive(Clone, Debug, PartialEq)]
pub enum ProbeTarget {
    /// A single-geometry cycle from `ghs-probe`.
    Cycle(ProbeCycle),
    /// A rectangular height-map grid.
    HeightMap(HeightMapGrid),
}

/// A plan plus the offset snapshot it was built against.
#[derive(Clone, Debug)]
pub struct ProbeSetup {
    /// Shared parameters.
    pub params: ProbeParams,
    /// Cycle geometry or height-map grid.
    pub target: ProbeTarget,
    /// Validated program.
    pub plan: ProbePlan,
    /// Height-map point/time estimate, for the preflight sheet.
    pub estimate: Option<HeightMapEstimate>,
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
    /// Computed outcome once a standard cycle completed.
    pub outcome: Option<ProbeOutcome>,
    /// Measured height map, complete or partial.
    ///
    /// `None` for standard cycles. Missing cells stay `None` inside the map;
    /// they are never interpolated.
    pub map: Option<HeightMap>,
    /// Result of the last explicit CSV export.
    pub export: Option<HeightMapExport>,
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
    /// A failed height-map point asked the shell to stop the job once.
    pub stop_requested: bool,
}

/// Written CSV result for a measured height map.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMapExport {
    /// Full path the app wrote.
    pub path: PathBuf,
    /// Data row count, or the write error.
    pub rows: Result<usize, String>,
}

impl ProbeRun {
    fn new(setup: ProbeSetup) -> Self {
        let checkpoints = probe_checkpoints(&setup.plan.program);
        ProbeRun {
            setup,
            results: Vec::new(),
            raw: Vec::new(),
            outcome: None,
            map: None,
            export: None,
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
            stop_requested: false,
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

    /// Whether this run measures a height map.
    #[must_use]
    pub fn is_height_map(&self) -> bool {
        matches!(self.setup.target, ProbeTarget::HeightMap(_))
    }
}

/// Validated height-map inputs, before the full G-code plan is built.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMapDraft {
    /// Grid in machine coordinates.
    pub grid: HeightMapGrid,
    /// Shared probe parameters.
    pub params: ProbeParams,
    /// Point/time estimate.
    pub estimate: HeightMapEstimate,
    /// Operator-entered grid origin in work coordinates.
    pub origin_work_mm: XyPoint,
}

/// One inline field problem for the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldProblem {
    /// Field to show the message under, when it maps to one.
    pub field: Option<ProbeField>,
    /// Operator-facing message.
    pub message: String,
}

/// The probe workspace model.
#[derive(Clone, Debug)]
pub struct ProbeModel {
    card: Option<ProbeCard>,
    fields: [String; ProbeField::COUNT],
    field_errors: [Option<String>; ProbeField::COUNT],
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
        let field_errors = std::array::from_fn(|_| None);
        let mut model = ProbeModel {
            card: None,
            fields,
            field_errors,
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

    /// Last inline validation error for a field, if any.
    #[must_use]
    pub fn field_error(&self, field: ProbeField) -> Option<&str> {
        self.field_errors
            .get(field.index())
            .and_then(Option::as_deref)
    }

    /// Validate and store one field from the shared keypad.
    ///
    /// # Errors
    ///
    /// Returns a message for a non-numeric, non-finite or out-of-range value.
    /// The stored text is left unchanged on error, but the message is retained
    /// so the editor can show it under the field.
    pub fn set_field_text(
        &mut self,
        field: ProbeField,
        text: impl Into<String>,
    ) -> Result<(), String> {
        let text = text.into();
        let trimmed = text.trim();
        let result = self.validate_field_text(field, trimmed);
        if let Some(slot) = self.field_errors.get_mut(field.index()) {
            *slot = result.as_ref().err().cloned();
        }
        if result.is_ok()
            && let Some(slot) = self.fields.get_mut(field.index())
        {
            *slot = trimmed.to_string();
        }
        result
    }

    fn validate_field_text(&self, field: ProbeField, trimmed: &str) -> Result<(), String> {
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
        if field.is_integer() && value.fract() != 0.0 {
            return Err(format!("{} must be a whole number", field.label()));
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
        let work_x_mm = ctx.work_pos_mm.get(CoreAxis::X);
        let work_y_mm = ctx.work_pos_mm.get(CoreAxis::Y);
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
            // The grid origin is entered in work coordinates and converted
            // with the active offset when the plan is built.
            (ProbeField::OriginX, work_x_mm),
            (ProbeField::OriginY, work_y_mm),
            (ProbeField::StepX, 10.0),
            (ProbeField::StepY, 10.0),
            // A conservative, self-consistent starting grid: safe Z is ten
            // millimetres up, the nominal surface is five below safe Z, and
            // the default search covers exactly that travel.
            (ProbeField::SurfaceZ, z_mm + 5.0),
        ];
        self.field_errors = std::array::from_fn(|_| None);
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
        if let Some(slot) = self.fields.get_mut(ProbeField::Columns.index()) {
            *slot = "4".to_string();
        }
        if let Some(slot) = self.fields.get_mut(ProbeField::Rows.index()) {
            *slot = "4".to_string();
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
            return self.prepare_height_map(ctx);
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
        // The plan is emitted verbatim: `ghs-probe` owns the G-code, and the
        // parser accepts its bare `G91` retract blocks after a modal `G38.2`.
        let plan = cycle.plan(&params).map_err(|error| error.to_string())?;
        Ok(ProbeSetup {
            params,
            target: ProbeTarget::Cycle(cycle),
            plan,
            estimate: None,
            wcs_value_mm: ctx.wcs_value_mm,
            g92_mm: ctx.g92_mm,
            tlo_z_mm: ctx.tlo_z_mm,
        })
    }

    /// Validate the height-map editor without building the G-code program.
    ///
    /// The UI calls this on every frame, so it deliberately stops at
    /// [`HeightMapGrid::validate_with_params`] and [`HeightMapGrid::estimate`];
    /// the full plan is built only after the operator presses RUN.
    ///
    /// # Errors
    ///
    /// Returns the first invalid field or the `ghs-probe` grid validation
    /// message.
    pub fn height_map_draft(&self, ctx: &ProbeContext) -> Result<HeightMapDraft, String> {
        let safe_z_mm = self.field_value(ProbeField::SafeZ)?;
        let search_distance_mm = self.field_value(ProbeField::Search)?;
        let feed_mm_min = self.field_value(ProbeField::Feed)?;
        let retract_mm = self.field_value(ProbeField::Retract)?;
        let probe_diameter_mm = self.field_value(ProbeField::Diameter)?;
        let origin_work_mm = XyPoint::new(
            self.field_value(ProbeField::OriginX)?,
            self.field_value(ProbeField::OriginY)?,
        );
        let step_x_mm = self.field_value(ProbeField::StepX)?;
        let step_y_mm = self.field_value(ProbeField::StepY)?;
        let columns = self.field_u16(ProbeField::Columns)?;
        let rows = self.field_u16(ProbeField::Rows)?;
        let surface_z_mm = self.field_value(ProbeField::SurfaceZ)?;
        let offset = ctx.offset_mm();
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(
                origin_work_mm.x_mm + offset.get(CoreAxis::X),
                origin_work_mm.y_mm + offset.get(CoreAxis::Y),
            ),
            step_x_mm,
            step_y_mm,
            columns,
            rows,
            surface_z_mm,
        };
        let params = ProbeParams {
            safe_z_mm,
            search_distance_mm,
            feed_mm_min,
            retract_mm,
            probe_diameter_mm,
            edge_offset_mm: 0.0,
            position: None,
            center: None,
            radius_mm: None,
            wcs_offset_mm: offset,
            wcs: ctx.wcs,
            units: Units::Mm,
        };
        // Explicit grid checks first, with messages that name the operator
        // fields. The crate repeats them during validation; this only makes
        // the inline editor and preflight reason clearer.
        if safe_z_mm <= surface_z_mm {
            return Err(format!(
                "SAFE Z {:.3} mm must be above SURFACE Z {:.3} mm",
                safe_z_mm, surface_z_mm
            ));
        }
        let travel_mm = safe_z_mm - surface_z_mm;
        if travel_mm > search_distance_mm {
            return Err(format!(
                "height-map travel {:.3} mm exceeds SEARCH {:.3} mm",
                travel_mm, search_distance_mm
            ));
        }
        grid.validate_with_params(&params)
            .map_err(|error| error.to_string())?;
        let estimate = grid.estimate(&params).map_err(|error| error.to_string())?;
        Ok(HeightMapDraft {
            grid,
            params,
            estimate,
            origin_work_mm,
        })
    }

    /// Validate inline field problems for the current card.
    ///
    /// Per-field syntax/range errors come from [`ProbeModel::set_field_text`];
    /// height-map cross-field errors are attached to the field named by the
    /// `ghs-probe` validation message.
    #[must_use]
    pub fn field_problems(&self, ctx: &ProbeContext) -> Vec<FieldProblem> {
        let mut problems = Vec::new();
        for field in ProbeField::ALL {
            if let Some(message) = self.field_error(field) {
                problems.push(FieldProblem {
                    field: Some(field),
                    message: message.to_string(),
                });
            }
        }
        if self.card == Some(ProbeCard::HeightMap)
            && let Err(message) = self.height_map_draft(ctx)
        {
            let field = height_map_error_field(&message);
            if !problems
                .iter()
                .any(|problem| problem.field == field && problem.message == message)
            {
                problems.push(FieldProblem { field, message });
            }
        }
        problems
    }

    /// Parsed height-map dimensions, even when the grid is not yet valid.
    #[must_use]
    pub fn height_map_point_count(&self) -> Option<(u16, u16, usize)> {
        let columns =
            u16::try_from(self.field_value(ProbeField::Columns).ok()?.round() as i64).ok()?;
        let rows = u16::try_from(self.field_value(ProbeField::Rows).ok()?.round() as i64).ok()?;
        let count = usize::from(columns).checked_mul(usize::from(rows))?;
        Some((columns, rows, count))
    }

    /// Measured height overlay in the viewer's work frame.
    ///
    /// Missing cells are included with `z_mm: None` so the overlay can mark
    /// them instead of silently filling them in.
    #[must_use]
    pub fn height_overlay(&self) -> Option<HeightOverlay> {
        let run = self.run.as_ref()?;
        let map = run.map.as_ref()?;
        Some(height_map_overlay(map, run.setup.params.wcs_offset_mm))
    }

    /// CSV text for the measured map, or an error when no map exists.
    ///
    /// # Errors
    ///
    /// Returns a message before a height-map run produced any cells.
    pub fn height_map_csv(&self) -> Result<String, String> {
        let run = self
            .run
            .as_ref()
            .ok_or_else(|| "no probe run to export".to_string())?;
        let map = run
            .map
            .as_ref()
            .ok_or_else(|| "this run has no measured height map".to_string())?;
        Ok(height_map_csv(map))
    }

    /// Record the result of an explicit CSV export.
    pub fn mark_height_export(&mut self, path: PathBuf, rows: Result<usize, String>) {
        if let Some(run) = self.run.as_mut() {
            run.export = Some(HeightMapExport { path, rows });
        }
    }

    /// Take a pending stop request from a failed height-map point.
    pub fn take_stop_request(&mut self) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        let requested = run.stop_requested;
        run.stop_requested = false;
        requested
    }

    fn prepare_height_map(&self, ctx: &ProbeContext) -> Result<ProbeSetup, String> {
        let draft = self.height_map_draft(ctx)?;
        let plan = draft
            .grid
            .plan(&draft.params)
            .map_err(|error| error.to_string())?;
        Ok(ProbeSetup {
            params: draft.params,
            target: ProbeTarget::HeightMap(draft.grid),
            plan,
            estimate: Some(draft.estimate),
            wcs_value_mm: ctx.wcs_value_mm,
            g92_mm: ctx.g92_mm,
            tlo_z_mm: ctx.tlo_z_mm,
        })
    }

    fn field_u16(&self, field: ProbeField) -> Result<u16, String> {
        let value = self.field_value(field)?;
        if value.fract() != 0.0 {
            return Err(format!("{} must be a whole number", field.label()));
        }
        u16::try_from(value.round() as i64)
            .map_err(|_| format!("{} is out of range", field.label()))
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
            ProbeCard::HeightMap => {
                return Err("height maps use the grid editor, not a cycle plan".to_string());
            }
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
                match &setup.target {
                    ProbeTarget::Cycle(cycle) => {
                        push(
                            "EXPECTED",
                            expected_text(cycle, &setup.params),
                            RowState::Info,
                        );
                    }
                    ProbeTarget::HeightMap(grid) => {
                        let travel = setup.params.safe_z_mm - grid.surface_z_mm;
                        push(
                            "GRID",
                            format!(
                                "{} cols x {} rows = {} points (max {})",
                                grid.columns,
                                grid.rows,
                                grid.point_count(),
                                MAX_HEIGHT_MAP_POINTS
                            ),
                            RowState::Ok,
                        );
                        push(
                            "SURFACE Z",
                            format!("{:.3} mm machine", grid.surface_z_mm),
                            RowState::Info,
                        );
                        push(
                            "GRID ORIGIN",
                            format!(
                                "X {:.3} Y {:.3} mm machine",
                                grid.origin_mm.x_mm, grid.origin_mm.y_mm
                            ),
                            RowState::Info,
                        );
                        push(
                            "MAP TRAVEL",
                            format!(
                                "{:.3} mm from safe Z (search {:.3} mm)",
                                travel, setup.params.search_distance_mm
                            ),
                            RowState::Ok,
                        );
                        if let Some(estimate) = &setup.estimate {
                            push("ESTIMATE", format_estimate(estimate), RowState::Info);
                        }
                        push(
                            "SAFE Z NOTE",
                            "safe Z must clear the highest surface point; every probe descends from it"
                                .to_string(),
                            RowState::Warn,
                        );
                        push(
                            "COMPENSATION",
                            "measurement only; the map is not applied to the job".to_string(),
                            RowState::Warn,
                        );
                    }
                }
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
        // A failed point ends the map. G38.2 normally raises a controller alarm
        // and aborts the job; the app must also refuse to resume its demo
        // checkpoint or bind any later point.
        if run.is_height_map() && (!result.success || !result.position.is_finite()) {
            let index = run.results.len().saturating_sub(1) + 1;
            run.phase = ProbePhase::Failed;
            run.error = Some(format!(
                "probe point {index} failed; the height map stops here and never continues"
            ));
            run.paused_after_probe = None;
            run.stop_requested = true;
            run.map = partial_height_map(&run.setup.target, &run.results);
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
        match &run.setup.target {
            ProbeTarget::Cycle(cycle) => {
                match ProbeOutcome::from_results(cycle, &run.setup.params, &run.results) {
                    Ok(outcome) => {
                        run.outcome = Some(outcome);
                        run.phase = ProbePhase::Complete;
                    }
                    Err(error) => {
                        run.phase = ProbePhase::Failed;
                        run.error = Some(error.to_string());
                    }
                }
            }
            ProbeTarget::HeightMap(grid) => {
                match HeightMap::from_results(grid.clone(), &run.results) {
                    Ok(map) => {
                        let complete = run.results.len() == run.setup.plan.expected_probes;
                        run.map = Some(map);
                        if complete {
                            run.phase = ProbePhase::Complete;
                        } else {
                            run.phase = ProbePhase::Failed;
                            run.error = Some(format!(
                                "job completed with {} of {} probe results; missing cells stay invalid",
                                run.results.len(),
                                run.setup.plan.expected_probes
                            ));
                        }
                    }
                    Err(error) => {
                        run.phase = ProbePhase::Failed;
                        run.error = Some(error.to_string());
                    }
                }
            }
        }
        true
    }

    /// Stop a run after a job error, abort, alarm, disconnect or watchdog.
    ///
    /// Only a running probe is failed; a value that already completed stays
    /// visible for audit, while Apply is gated by the live machine state.
    /// Keeps the partial raw results, builds a partial height map when cells
    /// were already measured, and never resumes.
    pub fn fail(&mut self, reason: impl Into<String>) -> bool {
        let reason = reason.into();
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        if run.phase != ProbePhase::Running {
            return false;
        }
        if run.is_height_map() && run.map.is_none() {
            run.map = partial_height_map(&run.setup.target, &run.results);
        }
        run.phase = ProbePhase::Failed;
        run.error = Some(reason);
        run.apply_confirm = false;
        run.paused_after_probe = None;
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

/// Build a partial height map from however many ordered results were bound.
fn partial_height_map(target: &ProbeTarget, results: &[ProbeResult]) -> Option<HeightMap> {
    let ProbeTarget::HeightMap(grid) = target else {
        return None;
    };
    HeightMap::from_results(grid.clone(), results).ok()
}

/// Counts, ranges and compensation spread for a measured map.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMapSummary {
    /// Total cells.
    pub total: usize,
    /// Valid measured cells.
    pub valid: usize,
    /// Cells without a valid measurement.
    pub missing: usize,
    /// Every cell has a measurement.
    pub complete: bool,
    /// Lowest measured machine Z.
    pub min_z_mm: Option<f64>,
    /// Highest measured machine Z.
    pub max_z_mm: Option<f64>,
    /// Lowest compensation (`measured - nominal surface`).
    pub min_compensation_mm: Option<f64>,
    /// Highest compensation (`measured - nominal surface`).
    pub max_compensation_mm: Option<f64>,
    /// Nominal surface Z from the grid.
    pub nominal_z_mm: f64,
}

/// Summarise a height map without filling any missing cell.
#[must_use]
pub fn height_map_summary(map: &HeightMap) -> HeightMapSummary {
    let (min_z, max_z) = map
        .z_range()
        .map_or((None, None), |(low, high)| (Some(low), Some(high)));
    let nominal = map.grid().surface_z_mm;
    let mut min_comp: Option<f64> = None;
    let mut max_comp: Option<f64> = None;
    for z in map.samples().iter().flatten() {
        let compensation = z - nominal;
        min_comp = Some(min_comp.map_or(compensation, |value| value.min(compensation)));
        max_comp = Some(max_comp.map_or(compensation, |value| value.max(compensation)));
    }
    HeightMapSummary {
        total: map.len(),
        valid: map.valid_count(),
        missing: map.missing_count(),
        complete: map.is_complete(),
        min_z_mm: min_z,
        max_z_mm: max_z,
        min_compensation_mm: min_comp,
        max_compensation_mm: max_comp,
        nominal_z_mm: nominal,
    }
}

/// CSV export of every cell in machine coordinates.
///
/// Rows are emitted for missing cells too, with an empty Z and `false` in the
/// validity column, so a consumer never mistakes a gap for a measurement.
#[must_use]
pub fn height_map_csv(map: &HeightMap) -> String {
    let grid = map.grid();
    let mut text = String::from(
        "column,row,machine_x_mm,machine_y_mm,measured_machine_z_mm,compensation_mm,valid\n",
    );
    for row in 0..grid.rows {
        for column in 0..grid.columns {
            let Some(point) = grid.point(column, row) else {
                continue;
            };
            let z = map.z(column, row);
            let (z_text, compensation_text, valid) = match z {
                Some(value) => (
                    format_mm(value),
                    format_mm(value - grid.surface_z_mm),
                    "true",
                ),
                None => (String::new(), String::new(), "false"),
            };
            text.push_str(&format!(
                "{column},{row},{},{},{z_text},{compensation_text},{valid}\n",
                format_mm(point.x_mm),
                format_mm(point.y_mm)
            ));
        }
    }
    text
}

/// Project a measured map into the viewer's work-coordinate overlay.
///
/// `offset_mm` is the active work offset at the run
/// (`machine = work + offset`). Every grid point becomes one cell of half a
/// step in each direction; missing cells are retained with `z_mm: None`.
#[must_use]
pub fn height_map_overlay(map: &HeightMap, offset_mm: Position) -> HeightOverlay {
    let grid = map.grid();
    let offset_x = offset_mm.get(CoreAxis::X);
    let offset_y = offset_mm.get(CoreAxis::Y);
    let half_x = grid.step_x_mm * 0.5;
    let half_y = grid.step_y_mm * 0.5;
    let mut cells = Vec::with_capacity(grid.point_count());
    for row in 0..grid.rows {
        for column in 0..grid.columns {
            let Some(point) = grid.point(column, row) else {
                continue;
            };
            let x = point.x_mm - offset_x;
            let y = point.y_mm - offset_y;
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            cells.push(HeightCell {
                min: Point2::new((x - half_x) as f32, (y - half_y) as f32),
                max: Point2::new((x + half_x) as f32, (y + half_y) as f32),
                z_mm: map.z(column, row),
            });
        }
    }
    let (min_z_mm, max_z_mm) = map
        .z_range()
        .unwrap_or((grid.surface_z_mm, grid.surface_z_mm));
    HeightOverlay {
        cells,
        min_z_mm,
        max_z_mm,
        nominal_z_mm: grid.surface_z_mm,
    }
}

/// Compact estimate text for the grid editor and preflight sheet.
#[must_use]
pub fn format_estimate(estimate: &HeightMapEstimate) -> String {
    format!(
        "{} points | {} mm probe travel | {}",
        estimate.points,
        format_mm(estimate.probing_distance_mm),
        format_duration(estimate.total_seconds)
    )
}

/// Format a duration in seconds as `m:ss`, clamped to non-negative input.
#[must_use]
pub fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.round().clamp(0.0, 359_999.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Best-effort mapping from a `ghs-probe` validation message to an editor
/// field, so a cross-field grid error can be shown inline.
fn height_map_error_field(message: &str) -> Option<ProbeField> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("grid") && lower.contains("exceeds") {
        return Some(ProbeField::Columns);
    }
    if lower.contains("columns") || lower.contains("point limit") {
        return Some(ProbeField::Columns);
    }
    if lower.contains("rows") {
        return Some(ProbeField::Rows);
    }
    if lower.contains("step_x") || lower.contains("grid x span") {
        return Some(ProbeField::StepX);
    }
    if lower.contains("step_y") || lower.contains("grid y span") {
        return Some(ProbeField::StepY);
    }
    if lower.contains("origin x") || lower.contains("grid maximum x") {
        return Some(ProbeField::OriginX);
    }
    if lower.contains("origin y") || lower.contains("grid maximum y") {
        return Some(ProbeField::OriginY);
    }
    if lower.contains("surface") {
        return Some(ProbeField::SurfaceZ);
    }
    if lower.contains("search") {
        return Some(ProbeField::Search);
    }
    if lower.contains("travel") || lower.contains("safe_z") {
        return Some(ProbeField::SafeZ);
    }
    if lower.contains("feed") {
        return Some(ProbeField::Feed);
    }
    if lower.contains("retract") {
        return Some(ProbeField::Retract);
    }
    if lower.contains("diameter") {
        return Some(ProbeField::Diameter);
    }
    None
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
        assert!(
            height.prepare(&ctx).is_ok(),
            "a default height grid must produce a valid plan"
        );
        assert!(ProbeCard::HeightMap.deferred_note().is_none());
    }

    fn set_height_grid(model: &mut ProbeModel, columns: u16, rows: u16) {
        for (field, text) in [
            (ProbeField::OriginX, "0.000"),
            (ProbeField::OriginY, "0.000"),
            (ProbeField::StepX, "10.000"),
            (ProbeField::StepY, "10.000"),
            (ProbeField::SurfaceZ, "0.000"),
            (ProbeField::SafeZ, "5.000"),
            (ProbeField::Search, "5.000"),
            (ProbeField::Feed, "100.000"),
            (ProbeField::Retract, "2.000"),
            (ProbeField::Diameter, "3.000"),
        ] {
            model
                .set_field_text(field, text)
                .unwrap_or_else(|error| panic!("{field:?}: {error}"));
        }
        model
            .set_field_text(ProbeField::Columns, columns.to_string())
            .expect("columns");
        model
            .set_field_text(ProbeField::Rows, rows.to_string())
            .expect("rows");
    }

    #[test]
    fn height_grid_validates_through_the_crate_and_shows_field_errors() {
        let ctx = context();
        let mut model = model(ProbeCard::HeightMap);
        set_height_grid(&mut model, 4, 4);

        let draft = model.height_map_draft(&ctx).expect("valid draft");
        assert_eq!(draft.grid.point_count(), 16);
        // work origin 0,0 + active offset (5+1, 6+2) = machine (6, 8).
        assert_eq!(draft.grid.origin_mm, XyPoint::new(6.0, 8.0));

        let setup = model.prepare(&ctx).expect("height plan");
        assert_eq!(setup.plan.expected_probes, 16);
        assert_eq!(setup.plan.name, "height-map");
        assert_eq!(
            setup
                .plan
                .probes
                .iter()
                .map(|probe| probe.label.as_str())
                .take(5)
                .collect::<Vec<_>>(),
            vec!["p0-c0", "p0-c1", "p0-c2", "p0-c3", "p1-c0"]
        );

        // A bad whole-number field keeps its old text and shows inline.
        assert!(model.set_field_text(ProbeField::Columns, "1").is_err());
        assert_eq!(model.field_text(ProbeField::Columns), "4");
        assert!(
            model
                .field_error(ProbeField::Columns)
                .is_some_and(|error| error.contains("COLUMNS"))
        );

        model
            .set_field_text(ProbeField::Columns, "65")
            .expect("per-axis bound allows 65");
        model
            .set_field_text(ProbeField::Rows, "64")
            .expect("per-axis bound allows 64");
        let error = model.prepare(&ctx).expect_err("4160 points is too many");
        assert!(error.contains("4096"), "unexpected message: {error}");
        assert!(model.field_problems(&ctx).iter().any(|problem| {
            problem.field == Some(ProbeField::Columns) && problem.message.contains("4096")
        }));

        model
            .set_field_text(ProbeField::Columns, "4")
            .expect("columns");
        model.set_field_text(ProbeField::Rows, "4").expect("rows");
        model
            .set_field_text(ProbeField::Search, "4.000")
            .expect("search");
        let error = model.prepare(&ctx).expect_err("travel exceeds search");
        assert!(error.contains("travel"), "unexpected message: {error}");

        model
            .set_field_text(ProbeField::SurfaceZ, "0.000")
            .expect("surface");
        model
            .set_field_text(ProbeField::SafeZ, "0.000")
            .expect("safe Z");
        let error = model
            .prepare(&ctx)
            .expect_err("safe Z is not above surface");
        assert!(
            error.contains("above SURFACE Z"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn height_estimate_text_is_compact_and_bounded() {
        let ctx = context();
        let mut model = model(ProbeCard::HeightMap);
        set_height_grid(&mut model, 4, 4);
        let draft = model.height_map_draft(&ctx).expect("draft");
        assert_eq!(draft.estimate.points, 16);
        assert_eq!(draft.estimate.probing_distance_mm, 80.0);
        assert_eq!(draft.estimate.total_seconds, 96.0);
        assert_eq!(
            format_estimate(&draft.estimate),
            "16 points | 80.000 mm probe travel | 1:36"
        );
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(-1.0), "0:00");
        assert_eq!(format_duration(f64::NAN), "0:00");
    }

    #[test]
    fn height_results_bind_in_order_and_a_stop_keeps_a_partial_map() {
        let ctx = context();
        let mut model = model(ProbeCard::HeightMap);
        set_height_grid(&mut model, 2, 2);
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("height-map"));
        assert!(model.on_job_started());
        // Machine points are work 0/0 and 10/0 plus the active offset (6, 8).
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(6.0, 8.0, 0.0))));
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(16.0, 8.0, -1.0))));
        model.fail("operator stopped the probe");

        let run = model.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Failed);
        let map = run.map.as_ref().expect("partial map");
        assert_eq!(map.valid_count(), 2);
        assert_eq!(map.missing_count(), 2);
        assert_eq!(map.z(0, 0), Some(0.0));
        assert_eq!(map.z(1, 0), Some(-1.0));
        assert_eq!(map.z(0, 1), None);
        // A missing row is not bridged, not even in the middle of a cell.
        assert_eq!(map.height_at(11.0, 13.0), None);
        assert_eq!(map.height_at(6.0, 18.0), None);
        assert_eq!(map.height_at(16.0, 8.0), Some(-1.0));
    }

    #[test]
    fn a_failed_height_point_stops_the_run_and_never_binds_later_points() {
        let ctx = context();
        let mut model = model(ProbeCard::HeightMap);
        set_height_grid(&mut model, 2, 2);
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("height-map"));
        assert!(model.on_job_started());
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(6.0, 8.0, 0.0))));
        assert!(model.bind_result(ProbeResult::new(false, Position::xyz(16.0, 8.0, -5.0))));
        assert!(
            model
                .run()
                .and_then(|run| run.error.as_deref())
                .is_some_and(|text| text.contains("stops here"))
        );
        assert!(model.take_stop_request());
        assert!(!model.take_stop_request(), "stop request is consumed once");
        assert!(!model.needs_resume());
        let run = model.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Failed);
        // The failed point is an invalid cell, and later PRB events are refused.
        assert_eq!(run.map.as_ref().and_then(|map| map.z(1, 0)), None);
        assert!(!model.bind_result(ProbeResult::new(true, Position::xyz(6.0, 18.0, 0.0))));
    }

    #[test]
    fn height_summary_and_overlay_mark_missing_cells_without_bridging() {
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(10.0, 20.0),
            step_x_mm: 4.0,
            step_y_mm: 6.0,
            columns: 2,
            rows: 2,
            surface_z_mm: 0.0,
        };
        let map = HeightMap::from_samples(grid, vec![Some(1.0), None, Some(3.0), Some(4.0)])
            .expect("map");
        let summary = height_map_summary(&map);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.valid, 3);
        assert_eq!(summary.missing, 1);
        assert!(!summary.complete);
        assert_eq!(summary.min_z_mm, Some(1.0));
        assert_eq!(summary.max_z_mm, Some(4.0));
        assert_eq!(summary.min_compensation_mm, Some(1.0));
        assert_eq!(summary.max_compensation_mm, Some(4.0));

        let overlay = height_map_overlay(&map, Position::xyz(5.0, 6.0, 3.0));
        assert_eq!(overlay.cells.len(), 4);
        // Machine (10,20) minus offset (5,6) = work (5,14); half a 4x6 step.
        assert_eq!(overlay.cells[0].min, Point2::new(3.0, 11.0));
        assert_eq!(overlay.cells[0].max, Point2::new(7.0, 17.0));
        assert_eq!(overlay.cells[0].z_mm, Some(1.0));
        assert_eq!(overlay.cells[1].z_mm, None);
        assert_eq!(overlay.cells[3].z_mm, Some(4.0));
        assert_eq!(overlay.cells[3].min, Point2::new(7.0, 17.0));
        assert_eq!(overlay.cells[3].max, Point2::new(11.0, 23.0));
    }

    #[test]
    fn height_preflight_refuses_bad_travel_count_and_busy_machine() {
        let ctx = context();
        let mut valid = model(ProbeCard::HeightMap);
        set_height_grid(&mut valid, 4, 4);
        let sheet = valid.preflight(&ctx);
        assert!(sheet.blockers.is_empty(), "{:?}", sheet.blockers);
        assert!(sheet.setup.is_some());
        let labels: Vec<&str> = sheet.rows.iter().map(|row| row.label).collect();
        for required in [
            "GRID",
            "SURFACE Z",
            "GRID ORIGIN",
            "MAP TRAVEL",
            "ESTIMATE",
            "SAFE Z NOTE",
            "COMPENSATION",
        ] {
            assert!(labels.contains(&required), "missing {required}");
        }
        assert!(
            sheet
                .rows
                .iter()
                .any(|row| row.label == "GRID" && row.value.contains("16 points"))
        );

        let mut too_long = model(ProbeCard::HeightMap);
        set_height_grid(&mut too_long, 4, 4);
        too_long
            .set_field_text(ProbeField::Search, "4.000")
            .expect("search");
        let refused = too_long.preflight(&ctx);
        assert!(refused.setup.is_none());
        assert!(
            refused
                .blockers
                .iter()
                .any(|reason| reason.contains("SEARCH"))
        );

        let mut too_many = model(ProbeCard::HeightMap);
        set_height_grid(&mut too_many, 65, 64);
        let refused = too_many.preflight(&ctx);
        assert!(refused.setup.is_none());
        assert!(
            refused
                .blockers
                .iter()
                .any(|reason| reason.contains("4096"))
        );

        let mut busy = model(ProbeCard::HeightMap);
        set_height_grid(&mut busy, 4, 4);
        let mut busy_ctx = context();
        busy_ctx.spindle_on = true;
        assert!(busy.preflight(&busy_ctx).setup.is_none());
        assert!(
            busy.preflight(&busy_ctx)
                .blockers
                .iter()
                .any(|reason| reason.contains("spindle"))
        );
    }

    #[test]
    fn height_csv_lists_every_cell_including_missing_ones() {
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(10.0, 20.0),
            step_x_mm: 4.0,
            step_y_mm: 6.0,
            columns: 2,
            rows: 2,
            surface_z_mm: 0.0,
        };
        let map = HeightMap::from_samples(grid, vec![Some(1.5), None, Some(3.0), Some(4.0)])
            .expect("map");
        let csv = height_map_csv(&map);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 5, "header plus one row per cell");
        assert_eq!(
            lines[0],
            "column,row,machine_x_mm,machine_y_mm,measured_machine_z_mm,compensation_mm,valid"
        );
        assert_eq!(lines[1], "0,0,10.000,20.000,1.500,1.500,true");
        assert_eq!(lines[2], "1,0,14.000,20.000,,,false");
        assert_eq!(lines[3], "0,1,10.000,26.000,3.000,3.000,true");
        assert_eq!(lines[4], "1,1,14.000,26.000,4.000,4.000,true");
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
            ProbeCard::HeightMap,
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
