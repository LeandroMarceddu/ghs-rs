//! Probe workspace model.
//!
//! This module owns the preflight inputs, the `ghs-probe` plan/result binding,
//! the `G10 L2` offset commands the shell sends after an explicit Apply, and
//! the confirmed `G43.1`/`G49` tool-length write model with its `$#`
//! verification. It holds no GPUI types and never talks to a transport, so the
//! safety gates, unit conversion, correlation and command formatting are
//! testable without a window or an engine.
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
//! the final authority; nothing here sends motion. A tool-length apply/clear
//! additionally requires a completed cycle, an explicit confirmation, an Idle
//! controller, and no other TLO write awaiting its `$#` read-back.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ghs_config::ProbeDefaultsRecord;
use ghs_core::Units;
use ghs_engine::{
    Axis as CoreAxis, Command, MachineState, Position, ProbeResult, SystemCommand, Wcs,
};
use ghs_probe::{
    CornerProbe, EdgeMode, EdgeSpec, HeightMap, HeightMapEstimate, HeightMapGrid,
    MAX_CIRCLE_TOUCHES, MAX_COORDINATE_MM, MAX_EDGE_OFFSET_MM, MAX_HEIGHT_MAP_AXIS_POINTS,
    MAX_HEIGHT_MAP_POINTS, MAX_HEIGHT_MAP_Z_OFFSET_MM, MAX_PLATE_THICKNESS_MM,
    MAX_PROBE_DIAMETER_MM, MAX_PROBE_FEED_MM_MIN, MAX_PROBE_OFFSET_MM, MAX_RADIUS_MM,
    MAX_RECTANGLE_MM, MAX_RETRACT_MM, MAX_SEARCH_DISTANCE_MM, MAX_SEPARATION_MM,
    MIN_CIRCLE_TOUCHES, ProbeAxis, ProbeCycle, ProbeOutcome, ProbeParams, ProbePlan, ProbeValue,
    RotationProbe, Sign, XyPoint, validate_probe_macro, validate_probe_offset,
};
use ghs_viewer::Point2;

use crate::state::Units as DisplayUnits;
use crate::viewer::{HeightCell, HeightOverlay};

/// Largest absolute dynamic tool-length offset accepted for `G43.1`.
///
/// The reference envelope is far smaller. The bound rejects a typo before any
/// command is built and keeps the emitted line comfortably bounded.
pub const MAX_TOOL_LENGTH_MM: f64 = 10_000.0;

/// Accepted difference between the requested TLO and the `$#` read-back.
pub const TOOL_LENGTH_READBACK_TOLERANCE_MM: f64 = 0.001;

/// How long a confirmed TLO write waits for its `$#` read-back.
pub const TOOL_LENGTH_READBACK_TIMEOUT: Duration = Duration::from_secs(3);

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
    /// X component of `probe tip - controlled point`, in millimetres.
    ProbeOffsetX,
    /// Y component of `probe tip - controlled point`, in millimetres.
    ProbeOffsetY,
    /// Constant datum shift added to every measured height, in millimetres.
    MapZOffset,
    /// Rectangle width for the centre cycle, in millimetres.
    RectangleWidth,
    /// Rectangle height for the centre cycle, in millimetres.
    RectangleHeight,
    /// X corner of the XY touch plate, in machine millimetres.
    PlateX,
    /// Y corner of the XY touch plate, in machine millimetres.
    PlateY,
}

impl ProbeField {
    /// Number of editable fields.
    pub const COUNT: usize = 31;

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
        ProbeField::ProbeOffsetX,
        ProbeField::ProbeOffsetY,
        ProbeField::MapZOffset,
        ProbeField::RectangleWidth,
        ProbeField::RectangleHeight,
        ProbeField::PlateX,
        ProbeField::PlateY,
    ];

    /// Shared fields persisted as per-controller probe defaults.
    ///
    /// Loading a profile prefills exactly these cells; every anchor and
    /// expected value stays per run.
    pub const PROFILE_DEFAULTS: [ProbeField; 7] = [
        ProbeField::SafeZ,
        ProbeField::Search,
        ProbeField::Feed,
        ProbeField::Retract,
        ProbeField::Diameter,
        ProbeField::EdgeOffset,
        ProbeField::PlateThickness,
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
            ProbeField::ProbeOffsetX => 24,
            ProbeField::ProbeOffsetY => 25,
            ProbeField::MapZOffset => 26,
            ProbeField::RectangleWidth => 27,
            ProbeField::RectangleHeight => 28,
            ProbeField::PlateX => 29,
            ProbeField::PlateY => 30,
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
            ProbeField::ProbeOffsetX => "PROBE OFFSET X",
            ProbeField::ProbeOffsetY => "PROBE OFFSET Y",
            ProbeField::MapZOffset => "MAP Z OFFSET",
            ProbeField::RectangleWidth => "RECT WIDTH",
            ProbeField::RectangleHeight => "RECT HEIGHT",
            ProbeField::PlateX => "PLATE CORNER X",
            ProbeField::PlateY => "PLATE CORNER Y",
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
            ProbeField::ProbeOffsetX => "probe-offset-x",
            ProbeField::ProbeOffsetY => "probe-offset-y",
            ProbeField::MapZOffset => "map-z-offset",
            ProbeField::RectangleWidth => "rectangle-width",
            ProbeField::RectangleHeight => "rectangle-height",
            ProbeField::PlateX => "plate-x",
            ProbeField::PlateY => "plate-y",
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
            ProbeField::RectangleWidth | ProbeField::RectangleHeight => (0.0, MAX_RECTANGLE_MM),
            ProbeField::Touches => (MIN_CIRCLE_TOUCHES as f64, MAX_CIRCLE_TOUCHES as f64),
            ProbeField::Separation => (0.0, MAX_SEPARATION_MM),
            ProbeField::StepX | ProbeField::StepY => (0.0, MAX_COORDINATE_MM),
            ProbeField::Columns | ProbeField::Rows => (2.0, MAX_HEIGHT_MAP_AXIS_POINTS as f64),
            ProbeField::ProbeOffsetX | ProbeField::ProbeOffsetY => {
                (-MAX_PROBE_OFFSET_MM, MAX_PROBE_OFFSET_MM)
            }
            ProbeField::MapZOffset => (-MAX_HEIGHT_MAP_Z_OFFSET_MM, MAX_HEIGHT_MAP_Z_OFFSET_MM),
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
            ProbeField::RectangleWidth => "60.000".to_string(),
            ProbeField::RectangleHeight => "40.000".to_string(),
            ProbeField::PlateX => "20.000".to_string(),
            ProbeField::PlateY => "20.000".to_string(),
            // A zero offset is the identity: the plan and result math are
            // exactly the pre-offset workflow.
            ProbeField::ProbeOffsetX | ProbeField::ProbeOffsetY | ProbeField::MapZOffset => {
                "0.000".to_string()
            }
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
    /// Rectangle centre.
    CenterRectangle,
    /// Two-point rotation.
    Rotation,
    /// XY touch plate corner.
    XyTouchPlate,
    /// Tool-length reference.
    ToolLength,
    /// Height map.
    HeightMap,
}

impl ProbeCard {
    /// Every card, in display order.
    pub const ALL: [ProbeCard; 10] = [
        ProbeCard::ZTouchPlate,
        ProbeCard::EdgeX,
        ProbeCard::EdgeY,
        ProbeCard::Corner,
        ProbeCard::CenterCircle,
        ProbeCard::CenterRectangle,
        ProbeCard::Rotation,
        ProbeCard::XyTouchPlate,
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
            ProbeCard::CenterRectangle => "Center rectangle",
            ProbeCard::Rotation => "Rotation",
            ProbeCard::XyTouchPlate => "XY touch plate",
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
            ProbeCard::CenterRectangle => {
                "Four side touches; fits centre, width and height from the edges."
            }
            ProbeCard::Rotation => "Two points on an X edge; reports angle, apply deferred.",
            ProbeCard::XyTouchPlate => {
                "Two outside plate edges; sets X/Y zero at the plate corner."
            }
            ProbeCard::ToolLength => {
                "Down onto the reference; reports delta and offers confirmed G43.1 TLO apply."
            }
            ProbeCard::HeightMap => {
                "Bounded Z grid; measured map overlay and explicit Apply/Revert."
            }
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
            ProbeCard::CenterRectangle => {
                "Touches the midpoint of each rectangle side, derives the four edges from the probe \
                 radius, and reports the fitted centre, measured width and measured height. A pocket \
                 probes outward from the inside; a boss probes inward from the outside."
            }
            ProbeCard::Rotation => {
                "Probes two points on a nominally X-aligned edge and reports the measured angle and \
                 deviation. Applying work rotation is controller/app dependent and deferred."
            }
            ProbeCard::XyTouchPlate => {
                "Probes the two outside edges of a conductive corner plate. The plate corner field is \
                 the feature corner; each search starts outside and moves toward the plate along the \
                 selected approach direction. The work zero is the corner plus the edge offset."
            }
            ProbeCard::ToolLength => {
                "Probes down onto a reference surface and reports the signed tool-length delta. \
                 APPLY TLO writes the absolute target with G43.1 and verifies the reported `$#` \
                 value; CLEAR TLO sends G49. Neither action is automatic."
            }
            ProbeCard::HeightMap => {
                "Row-major grid of Z touches with a retract between points. Missing cells stay \
                 invalid; Apply rewrites the loaded program and keeps the original for Revert."
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
            ProbeCard::CenterRectangle => &[
                // Geometry first so the rectangle-specific inputs are visible
                // without scrolling past the shared search fields.
                ProbeField::CenterX,
                ProbeField::CenterY,
                ProbeField::ProbingHeight,
                ProbeField::RectangleWidth,
                ProbeField::RectangleHeight,
                ProbeField::Diameter,
                ProbeField::EdgeOffset,
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
            ],
            ProbeCard::XyTouchPlate => &[
                ProbeField::PlateX,
                ProbeField::PlateY,
                ProbeField::ProbingHeight,
                ProbeField::EdgeOffset,
                ProbeField::Diameter,
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
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
                // The datum shift is the first card field so it is visible
                // without scrolling past the whole grid editor.
                ProbeField::MapZOffset,
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

/// One confirmed dynamic tool-length write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolLengthAction {
    /// Write the proposed absolute TLO with `G43.1`.
    Apply,
    /// Cancel the TLO with `G49`.
    Clear,
}

impl ToolLengthAction {
    /// Button label before the confirmation step.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ToolLengthAction::Apply => "APPLY TLO",
            ToolLengthAction::Clear => "CLEAR TLO",
        }
    }

    /// Confirmation button/title label.
    #[must_use]
    pub const fn confirm_label(self) -> &'static str {
        match self {
            ToolLengthAction::Apply => "CONFIRM APPLY TLO",
            ToolLengthAction::Clear => "CONFIRM CLEAR TLO",
        }
    }
}

/// Computed TLO values for the completed tool-length cycle.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolLengthPreview {
    /// TLO captured when the run started, in millimetres.
    pub current_tlo_mm: f64,
    /// Measured signed delta (`contact - reference`), in millimetres.
    pub delta_mm: f64,
    /// Proposed absolute TLO (`current + delta`), in millimetres.
    pub target_tlo_mm: f64,
}

impl ToolLengthPreview {
    /// Exact `G43.1` line for the proposed target.
    #[must_use]
    pub fn apply_line(&self) -> String {
        format!("G43.1 Z{}", format_mm(self.target_tlo_mm))
    }

    /// Exact line that clears the dynamic TLO.
    #[must_use]
    pub const fn clear_line() -> &'static str {
        "G49"
    }
}

/// Controller preconditions for a TLO apply or clear.
///
/// The completed run, confirmation, and single-write rule live in
/// [`ProbeModel`]; this gate covers the live machine state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolLengthGate {
    /// A live engine session is open (`--mock` is refused).
    pub live: bool,
    /// The handshake completed.
    pub ready: bool,
    /// Current controller state.
    pub machine_state: MachineState,
    /// A job runner is feeding or paused.
    pub job_active: bool,
    /// Another offset write is waiting for its `$#` read-back.
    pub offsets_pending: bool,
}

impl ToolLengthGate {
    /// Validate the gate and return the first refusal reason.
    ///
    /// # Errors
    ///
    /// Returns a message for mock/no link, not ready, Alarm/Door, an active
    /// job, a non-Idle machine, or another offset write in flight.
    pub fn check(self) -> Result<(), String> {
        if !self.live {
            return Err("a live controller is required; mock mode has no TLO write".to_string());
        }
        if !self.ready {
            return Err("the controller is not ready".to_string());
        }
        if matches!(self.machine_state, MachineState::Alarm | MachineState::Door) {
            return Err(format!(
                "the controller is in {}; recover before a TLO write",
                self.machine_state.as_str()
            ));
        }
        if self.job_active {
            return Err("a job is active".to_string());
        }
        if self.machine_state != MachineState::Idle {
            return Err("the controller must be Idle".to_string());
        }
        if self.offsets_pending {
            return Err("another offset write is awaiting `$#`".to_string());
        }
        Ok(())
    }
}

/// Lifecycle of one confirmed TLO write.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolLengthWriteStatus {
    /// The line was accepted; waiting for the `$#` TLO read-back.
    AwaitingReadBack,
    /// `$#` reported the requested value.
    Applied {
        /// Reported TLO on Z, in millimetres.
        reported_tlo_mm: f64,
    },
    /// The write was refused, unverified, mismatched, timed out or interrupted.
    Failed {
        /// Operator-facing reason.
        message: String,
    },
}

impl ToolLengthWriteStatus {
    /// Whether the write is still waiting for its read-back.
    #[must_use]
    pub fn is_awaiting_read_back(&self) -> bool {
        matches!(self, ToolLengthWriteStatus::AwaitingReadBack)
    }
}

/// One confirmed TLO line and its verification state.
#[derive(Clone, Debug)]
pub struct ToolLengthWrite {
    /// Action that produced the line.
    pub action: ToolLengthAction,
    /// Exact MDI line sent.
    pub line: String,
    /// Absolute target TLO in millimetres; zero for `G49`.
    pub target_tlo_mm: f64,
    /// Verification lifecycle.
    pub status: ToolLengthWriteStatus,
    /// When the line was accepted, for the read-back deadline.
    started_at: Instant,
}

impl ToolLengthWrite {
    /// Whether this write still blocks another TLO write.
    #[must_use]
    pub fn is_in_flight(&self) -> bool {
        self.status.is_awaiting_read_back()
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
    /// Physical probe mount offset, `probe tip - controlled point`, in
    /// millimetres. The plan commands `feature - offset`; result math adds it
    /// back before computing feature coordinates.
    pub probe_offset: XyPoint,
    /// Constant datum shift captured for a height map, in millimetres.
    ///
    /// It is applied only when the map is built from results. It never enters
    /// the probe plan or the `surface_z_mm` search target and is zero for
    /// standard cycles.
    pub map_z_offset_mm: f64,
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
    /// The operator is confirming one TLO action.
    pub tool_length_confirm: Option<ToolLengthAction>,
    /// The last TLO write and its `$#` verification state.
    pub tool_length_write: Option<ToolLengthWrite>,
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
            tool_length_confirm: None,
            tool_length_write: None,
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
    /// Rectangle is a pocket (`Inside`) or a boss (`Outside`).
    pub rectangle_mode: EdgeMode,
    /// Circle touch count, `3..=8`.
    pub circle_touches: u8,
    /// Rotation approach along Y for a nominally X-aligned edge.
    pub rotation_approach: Sign,
    /// X approach for the XY touch plate (the plate lies toward it).
    pub plate_x_approach: Sign,
    /// Y approach for the XY touch plate (the plate lies toward it).
    pub plate_y_approach: Sign,
    run: Option<ProbeRun>,
    /// Name of the most recent probe program loaded into the job runner.
    last_probe_program: Option<String>,
    /// A controller profile supplied the shared fields, so opening another
    /// card must not replace them with pose-derived factory values.
    defaults_loaded: bool,
    /// Validated operator lines inserted after the fixed safety preamble and
    /// before the first probe move.
    pre_macro: String,
    /// Validated operator lines appended after the final retract and safe-Z
    /// rapid.
    post_macro: String,
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
            rectangle_mode: EdgeMode::Inside,
            circle_touches: 4,
            rotation_approach: Sign::Negative,
            plate_x_approach: Sign::Positive,
            plate_y_approach: Sign::Positive,
            run: None,
            last_probe_program: None,
            defaults_loaded: false,
            pre_macro: String::new(),
            post_macro: String::new(),
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

    /// Validated pre-probe macro text, one cleaned line per `\n`.
    #[must_use]
    pub fn pre_macro(&self) -> &str {
        &self.pre_macro
    }

    /// Validated post-probe macro text, one cleaned line per `\n`.
    #[must_use]
    pub fn post_macro(&self) -> &str {
        &self.post_macro
    }

    /// Validate and store both operator macros.
    ///
    /// Each macro is trimmed line by line, blank lines are dropped, and the
    /// accepted lines are joined with `\n`. The stored text is exactly what
    /// [`ghs_probe::validate_probe_macro`] accepted, so a later
    /// [`ProbePlan::with_macros`] cannot change it.
    ///
    /// # Errors
    ///
    /// Returns the unchanged `ghs-probe` error text (including the one-based
    /// line number and rejected text) for a forbidden line, an out-of-range
    /// dwell, too many bytes, or too many lines. Neither macro changes when
    /// either text is refused.
    pub fn set_probe_macros(&mut self, pre: &str, post: &str) -> Result<(), String> {
        let pre_lines = validate_probe_macro(pre).map_err(|error| error.to_string())?;
        let post_lines = validate_probe_macro(post).map_err(|error| error.to_string())?;
        self.pre_macro = pre_lines.join("\n");
        self.post_macro = post_lines.join("\n");
        Ok(())
    }

    /// Short summary for the Probe workspace bar.
    ///
    /// Reports the number of cleaned pre and post lines, or `EMPTY` when both
    /// macros are empty.
    #[must_use]
    pub fn probe_macro_summary(&self) -> String {
        let pre = self
            .pre_macro
            .lines()
            .filter(|line| !line.is_empty())
            .count();
        let post = self
            .post_macro
            .lines()
            .filter(|line| !line.is_empty())
            .count();
        if pre == 0 && post == 0 {
            "EMPTY".to_string()
        } else {
            format!("PRE {pre} / POST {post} LINES")
        }
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

    /// Validated probe mount offset, `probe tip - controlled point`.
    ///
    /// # Errors
    ///
    /// Returns a message for an empty/non-numeric field, a non-finite value,
    /// or a component outside `+/- MAX_PROBE_OFFSET_MM`.
    pub fn probe_offset(&self) -> Result<XyPoint, String> {
        let offset = XyPoint::new(
            self.field_value(ProbeField::ProbeOffsetX)?,
            self.field_value(ProbeField::ProbeOffsetY)?,
        );
        validate_probe_offset(offset).map_err(|error| error.to_string())?;
        Ok(offset)
    }

    /// Store a validated probe mount offset in the editor fields.
    ///
    /// Used when a controller profile loads. A bad value is refused before
    /// either field changes, so the editor never holds half an offset.
    ///
    /// # Errors
    ///
    /// Returns the `ghs-probe` validation message for a non-finite or
    /// out-of-range component.
    pub fn set_probe_offset(&mut self, offset: XyPoint) -> Result<(), String> {
        validate_probe_offset(offset).map_err(|error| error.to_string())?;
        self.set_field_text(ProbeField::ProbeOffsetX, format!("{:.3}", offset.x_mm))?;
        self.set_field_text(ProbeField::ProbeOffsetY, format!("{:.3}", offset.y_mm))
    }

    /// Prefill the shared fields from one controller profile's defaults.
    ///
    /// Only [`ProbeField::PROFILE_DEFAULTS`] change, and their inline errors
    /// are cleared. Anchors, expected values, the probe mount offset, and the
    /// map Z offset are left exactly as the operator entered them. Both probe
    /// macros load with the numeric defaults; an invalid macro text refuses the
    /// whole record and changes nothing. An active probe run is never
    /// overwritten.
    ///
    /// Returns `false` when a run is active or the record is invalid.
    pub fn apply_defaults(&mut self, defaults: &ProbeDefaultsRecord) -> bool {
        if self.is_running() || defaults.validate().is_err() {
            return false;
        }
        let Ok(pre_macro) = validate_probe_macro(&defaults.pre_probe_macro) else {
            return false;
        };
        let Ok(post_macro) = validate_probe_macro(&defaults.post_probe_macro) else {
            return false;
        };
        for (field, value) in [
            (ProbeField::SafeZ, defaults.safe_z_mm),
            (ProbeField::Search, defaults.search_distance_mm),
            (ProbeField::Feed, defaults.feed_mm_min),
            (ProbeField::Retract, defaults.retract_mm),
            (ProbeField::Diameter, defaults.probe_diameter_mm),
            (ProbeField::EdgeOffset, defaults.edge_offset_mm),
            (ProbeField::PlateThickness, defaults.plate_thickness_mm),
        ] {
            if let Some(slot) = self.fields.get_mut(field.index()) {
                *slot = format_mm(value);
            }
            if let Some(error) = self.field_errors.get_mut(field.index()) {
                *error = None;
            }
        }
        self.pre_macro = pre_macro.join("\n");
        self.post_macro = post_macro.join("\n");
        self.defaults_loaded = true;
        true
    }

    /// Parse and validate the current shared field text as a defaults record.
    ///
    /// # Errors
    ///
    /// Returns a field-named message for an empty, non-numeric, non-finite,
    /// out-of-range, or non-positive search/feed/retract value. The stored
    /// field text is never changed.
    pub fn defaults_record(&self) -> Result<ProbeDefaultsRecord, String> {
        let record = ProbeDefaultsRecord {
            safe_z_mm: self.defaults_field(ProbeField::SafeZ)?,
            search_distance_mm: self.defaults_positive_field(ProbeField::Search)?,
            feed_mm_min: self.defaults_positive_field(ProbeField::Feed)?,
            retract_mm: self.defaults_positive_field(ProbeField::Retract)?,
            probe_diameter_mm: self.defaults_field(ProbeField::Diameter)?,
            edge_offset_mm: self.defaults_field(ProbeField::EdgeOffset)?,
            plate_thickness_mm: self.defaults_field(ProbeField::PlateThickness)?,
            // The model stores only text accepted by the probe-crate
            // allowlist, so the record carries the operator's exact macros.
            pre_probe_macro: self.pre_macro.clone(),
            post_probe_macro: self.post_macro.clone(),
        };
        record
            .validate()
            .map_err(|error| format!("probe defaults are invalid: {error}"))?;
        Ok(record)
    }

    fn defaults_field(&self, field: ProbeField) -> Result<f64, String> {
        let text = self.field_text(field).trim();
        // Validate first so syntax/range errors name the operator field.
        self.validate_field_text(field, text)?;
        self.field_value(field)
    }

    fn defaults_positive_field(&self, field: ProbeField) -> Result<f64, String> {
        let value = self.defaults_field(field)?;
        if value <= 0.0 {
            return Err(format!(
                "{} must be greater than 0.000 {}",
                field.label(),
                field.unit()
            ));
        }
        Ok(value)
    }

    /// Validated constant datum shift added to every measured height.
    ///
    /// This value is captured in [`ProbeSetup`] and applied to the map only;
    /// it never changes the probing plan or `surface_z_mm`.
    ///
    /// # Errors
    ///
    /// Returns a message for an empty/non-numeric field, a non-finite value,
    /// or a magnitude above [`MAX_HEIGHT_MAP_Z_OFFSET_MM`].
    pub fn map_z_offset_mm(&self) -> Result<f64, String> {
        let value = self.field_value(ProbeField::MapZOffset)?;
        if value.abs() > MAX_HEIGHT_MAP_Z_OFFSET_MM {
            return Err(format!(
                "{} must be within -{:.3} ..= {:.3} mm",
                ProbeField::MapZOffset.label(),
                MAX_HEIGHT_MAP_Z_OFFSET_MM,
                MAX_HEIGHT_MAP_Z_OFFSET_MM
            ));
        }
        Ok(value)
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
        // With a loaded profile the safe-Z default is authoritative, so keep
        // the nominal height-map surface at least five millimetres below it.
        // The pose-derived seed still wins when it is already lower.
        let surface_z_mm = if self.defaults_loaded {
            let safe_z_mm = self.field_value(ProbeField::SafeZ).unwrap_or(z_mm + 10.0);
            (z_mm + 5.0).min(safe_z_mm - 5.0)
        } else {
            z_mm + 5.0
        };
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
            (ProbeField::RectangleWidth, 60.0),
            (ProbeField::RectangleHeight, 40.0),
            (ProbeField::PlateX, x_mm + 20.0),
            (ProbeField::PlateY, y_mm + 20.0),
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
            (ProbeField::SurfaceZ, surface_z_mm),
        ];
        self.field_errors = std::array::from_fn(|_| None);
        for (field, value) in seeds {
            // A loaded controller profile owns the shared search/feed/retract
            // values. Opening a card only reseeds the per-run anchors.
            if self.defaults_loaded && ProbeField::PROFILE_DEFAULTS.contains(&field) {
                continue;
            }
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
    /// Ignored while a probe run is active or a TLO write is waiting for its
    /// `$#` read-back: a new card must never replace the plan whose result or
    /// write is still being resolved.
    pub fn select_card(&mut self, card: ProbeCard, ctx: &ProbeContext) -> bool {
        if self.is_running() || self.tool_length_write_in_flight() {
            return false;
        }
        self.card = Some(card);
        self.run = None;
        self.reseed(ctx);
        true
    }

    /// Return to the card grid. Ignored while a run or TLO write is active.
    pub fn close_card(&mut self) -> bool {
        if self.is_running() || self.tool_length_write_in_flight() {
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
        let probe_offset = self.probe_offset()?;
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
        // With a mount offset the cycle geometry stays in feature/tip
        // coordinates and every commanded XY move is `feature - offset`.
        let plan = cycle
            .plan_with_offset(&params, probe_offset)
            .map_err(|error| error.to_string())?;
        // The plan is validated, then the operator macros are inserted around
        // its probe moves. An empty macro is the identity path.
        let plan = plan
            .with_macros(&self.pre_macro, &self.post_macro)
            .map_err(|error| error.to_string())?;
        Ok(ProbeSetup {
            params,
            target: ProbeTarget::Cycle(cycle),
            plan,
            estimate: None,
            wcs_value_mm: ctx.wcs_value_mm,
            g92_mm: ctx.g92_mm,
            tlo_z_mm: ctx.tlo_z_mm,
            probe_offset,
            map_z_offset_mm: 0.0,
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
        // The datum shift is validated with the grid editor but is applied
        // only to the measured map; it never changes the plan or search target.
        let _map_z_offset_mm = self.map_z_offset_mm()?;
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
        if matches!(
            self.card,
            Some(ProbeCard::CenterRectangle | ProbeCard::XyTouchPlate)
        ) && let Err(message) = self.prepare(ctx)
        {
            let field = cycle_error_field(&message);
            if field.is_some()
                && !problems
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
        let probe_offset = self.probe_offset()?;
        let map_z_offset_mm = self.map_z_offset_mm()?;
        let plan = draft
            .grid
            .plan_with_offset(&draft.params, probe_offset)
            .map_err(|error| error.to_string())?;
        let plan = plan
            .with_macros(&self.pre_macro, &self.post_macro)
            .map_err(|error| error.to_string())?;
        Ok(ProbeSetup {
            params: draft.params,
            target: ProbeTarget::HeightMap(draft.grid),
            plan,
            estimate: Some(draft.estimate),
            wcs_value_mm: ctx.wcs_value_mm,
            g92_mm: ctx.g92_mm,
            tlo_z_mm: ctx.tlo_z_mm,
            probe_offset,
            map_z_offset_mm,
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
            ProbeCard::CenterRectangle => {
                let cx = self.field_value(ProbeField::CenterX)?;
                let cy = self.field_value(ProbeField::CenterY)?;
                (None, Some(Position::xyz(cx, cy, height)), None)
            }
            ProbeCard::XyTouchPlate => {
                let corner_x = self.field_value(ProbeField::PlateX)?;
                let corner_y = self.field_value(ProbeField::PlateY)?;
                let probe_radius = self.field_value(ProbeField::Diameter)? * 0.5;
                let search = self.field_value(ProbeField::Search)?;
                // Probe each edge in from the corner by the search corridor
                // plus the tip radius, so the probe line is inside the plate
                // body and a rounded corner cannot trigger the other axis.
                let inboard = probe_radius + search;
                let anchor_x = corner_x + self.plate_x_approach.as_f64() * inboard;
                let anchor_y = corner_y + self.plate_y_approach.as_f64() * inboard;
                (Some(Position::xyz(anchor_x, anchor_y, height)), None, None)
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
            ProbeCard::CenterRectangle => ProbeCycle::CenterRectangle {
                width_mm: self.field_value(ProbeField::RectangleWidth)?,
                height_mm: self.field_value(ProbeField::RectangleHeight)?,
                mode: self.rectangle_mode,
            },
            ProbeCard::XyTouchPlate => ProbeCycle::XyTouchPlate(CornerProbe {
                // A plate is a solid feature: both edges are approached from
                // outside toward the plate, using the selected directions.
                x: EdgeSpec {
                    axis: ProbeAxis::X,
                    mode: EdgeMode::Outside,
                    approach: self.plate_x_approach,
                    expected_edge_mm: self.field_value(ProbeField::PlateX)?,
                },
                y: EdgeSpec {
                    axis: ProbeAxis::Y,
                    mode: EdgeMode::Outside,
                    approach: self.plate_y_approach,
                    expected_edge_mm: self.field_value(ProbeField::PlateY)?,
                },
            }),
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
        match self.probe_offset() {
            Ok(offset) => push(
                "PROBE OFFSET",
                format!(
                    "X{} Y{} mm (probe tip - controlled point)",
                    format_mm(offset.x_mm),
                    format_mm(offset.y_mm)
                ),
                if offset.x_mm == 0.0 && offset.y_mm == 0.0 {
                    RowState::Info
                } else {
                    RowState::Warn
                },
            ),
            Err(error) => push("PROBE OFFSET", error, RowState::Bad),
        }
        push(
            "COORDINATES",
            "fields = feature/tip XY; plan/probes = controlled point = feature - offset"
                .to_string(),
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
                            "MAP Z OFFSET",
                            format!(
                                "{:.3} mm effective only; plan/search unchanged",
                                setup.map_z_offset_mm
                            ),
                            if setup.map_z_offset_mm == 0.0 {
                                RowState::Info
                            } else {
                                RowState::Warn
                            },
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
                            "explicit Apply after a complete map; the probe run never applies it"
                                .to_string(),
                            RowState::Info,
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
            run.map = partial_height_map(&run.setup, &run.results);
        } else if !result.success || !result.position.is_finite() {
            // A standard cycle has no partial value: a missed/alarmed search
            // stops the run immediately, asks the shell to stop the job, and
            // never resumes at a probe checkpoint.
            let index = run.results.len().saturating_sub(1) + 1;
            run.phase = ProbePhase::Failed;
            run.error = Some(format!(
                "probe {index} reported failure; the cycle stops and is never resumed"
            ));
            run.paused_after_probe = None;
            run.stop_requested = true;
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
                match ProbeOutcome::from_results_with_offset(
                    cycle,
                    &run.setup.params,
                    &run.results,
                    run.setup.probe_offset,
                ) {
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
                match HeightMap::from_results(grid.clone(), &run.results)
                    .and_then(|map| map.with_z_offset(run.setup.map_z_offset_mm))
                {
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
            run.map = partial_height_map(&run.setup, &run.results);
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
        let probe_failed = self.fail(format!(
            "ALARM:{code}; recover manually before another probe"
        ));
        let tlo_failed =
            self.fail_tool_length_write(format!("ALARM:{code}; the TLO write was not verified"));
        probe_failed || tlo_failed
    }

    /// A controller error event.
    ///
    /// A completed probe value stays for audit, but a TLO write waiting for
    /// `$#` can no longer be trusted.
    pub fn on_controller_error(&mut self, code: u16) -> bool {
        self.fail_tool_length_write(format!(
            "controller error:{code}; the TLO write was not verified"
        ))
    }

    /// A disconnect event.
    pub fn on_disconnected(&mut self) -> bool {
        let probe_failed = self.fail("disconnected; telemetry and offsets are stale".to_string());
        let tlo_failed =
            self.fail_tool_length_write("disconnected; the TLO write was not verified".to_string());
        probe_failed || tlo_failed
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

    /// TLO values for the completed tool-length cycle.
    ///
    /// `current_tlo_mm` is the TLO captured when the run started,
    /// `delta_mm` is the measured `contact - reference`, and
    /// `target_tlo_mm` is their sum rounded to the millimetre formatting used
    /// by the emitted line.
    ///
    /// # Errors
    ///
    /// Returns a message when the run is incomplete or not a tool-length cycle,
    /// when the current TLO or delta is non-finite, or when the target is
    /// outside `+/-` [`MAX_TOOL_LENGTH_MM`].
    pub fn tool_length_preview(&self) -> Result<ToolLengthPreview, String> {
        let run = self
            .run
            .as_ref()
            .ok_or_else(|| "no probe run to apply".to_string())?;
        let (current_tlo_mm, delta_mm) = tool_length_parts(run)?;
        if !current_tlo_mm.is_finite() {
            return Err("the current TLO is not finite".to_string());
        }
        if !delta_mm.is_finite() {
            return Err("the tool-length delta is not finite".to_string());
        }
        let target_tlo_mm = round_mm(current_tlo_mm + delta_mm);
        if !target_tlo_mm.is_finite() {
            return Err("the target TLO is not finite".to_string());
        }
        if target_tlo_mm.abs() > MAX_TOOL_LENGTH_MM {
            return Err(format!(
                "the target TLO {} mm is outside +/-{:.0} mm",
                format_mm(target_tlo_mm),
                MAX_TOOL_LENGTH_MM
            ));
        }
        Ok(ToolLengthPreview {
            current_tlo_mm,
            delta_mm,
            target_tlo_mm,
        })
    }

    /// Exact `G49` line for the completed tool-length cycle.
    ///
    /// Clearing does not depend on the proposed target, so it stays available
    /// when the measured delta cannot produce a valid `G43.1` target.
    ///
    /// # Errors
    ///
    /// Returns a message when the run is incomplete or not a tool-length cycle.
    pub fn tool_length_clear_line(&self) -> Result<String, String> {
        let run = self
            .run
            .as_ref()
            .ok_or_else(|| "no probe run to apply".to_string())?;
        let _ = tool_length_parts(run)?;
        Ok(ToolLengthPreview::clear_line().to_string())
    }

    /// TLO action currently waiting for the operator's confirmation.
    #[must_use]
    pub fn tool_length_confirmation(&self) -> Option<ToolLengthAction> {
        self.run.as_ref().and_then(|run| run.tool_length_confirm)
    }

    /// Last TLO write, including its verification state.
    #[must_use]
    pub fn tool_length_write(&self) -> Option<&ToolLengthWrite> {
        self.run
            .as_ref()
            .and_then(|run| run.tool_length_write.as_ref())
    }

    /// Whether a TLO write is still waiting for its `$#` read-back.
    #[must_use]
    pub fn tool_length_write_in_flight(&self) -> bool {
        self.tool_length_write()
            .is_some_and(ToolLengthWrite::is_in_flight)
    }

    /// Open the inline confirmation for one TLO action. Nothing is sent.
    ///
    /// # Errors
    ///
    /// Returns a message when the run is incomplete, the action cannot be
    /// built, or another TLO write is still awaiting its read-back.
    pub fn request_tool_length_action(&mut self, action: ToolLengthAction) -> Result<(), String> {
        match action {
            ToolLengthAction::Apply => {
                let _ = self.tool_length_preview()?;
            }
            ToolLengthAction::Clear => {
                let _ = self.tool_length_clear_line()?;
            }
        }
        let run = self
            .run
            .as_mut()
            .ok_or_else(|| "no probe run to apply".to_string())?;
        if run
            .tool_length_write
            .as_ref()
            .is_some_and(ToolLengthWrite::is_in_flight)
        {
            return Err("a TLO write is already awaiting its `$#` read-back".to_string());
        }
        run.tool_length_confirm = Some(action);
        run.kept = false;
        Ok(())
    }

    /// Cancel the inline TLO confirmation. Nothing is sent.
    ///
    /// Returns `true` when a confirmation was open.
    pub fn cancel_tool_length_action(&mut self) -> bool {
        self.run
            .as_mut()
            .and_then(|run| run.tool_length_confirm.take())
            .is_some()
    }

    /// Consume the confirmed TLO action, send exactly one MDI line and one
    /// `$#` request, then wait for the verified read-back.
    ///
    /// A gate refusal or a missing confirmation sends nothing. A sender
    /// rejection is recorded as `FAILED`; no retry is scheduled.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal, a missing confirmation, a build error, or the
    /// sender rejection.
    pub fn confirm_tool_length_action(
        &mut self,
        gate: ToolLengthGate,
        mut send: impl FnMut(Command) -> Result<(), String>,
    ) -> Result<ToolLengthWrite, String> {
        gate.check()?;
        let (action, in_flight) = {
            let run = self
                .run
                .as_ref()
                .ok_or_else(|| "no probe run to apply".to_string())?;
            if run.phase != ProbePhase::Complete {
                return Err("the probe run has not completed".to_string());
            }
            let action = run
                .tool_length_confirm
                .ok_or_else(|| "confirm the TLO action before sending it".to_string())?;
            let in_flight = run
                .tool_length_write
                .as_ref()
                .is_some_and(ToolLengthWrite::is_in_flight);
            (action, in_flight)
        };
        if in_flight {
            return Err("a TLO write is already awaiting its `$#` read-back".to_string());
        }
        let (line, target_tlo_mm) = match action {
            ToolLengthAction::Apply => {
                let preview = self.tool_length_preview()?;
                (preview.apply_line(), preview.target_tlo_mm)
            }
            ToolLengthAction::Clear => (self.tool_length_clear_line()?, 0.0),
        };
        if let Err(error) = send(Command::Mdi(line.clone())) {
            let message = format!("the engine rejected {line}: {error}");
            self.record_tool_length_failure(action, line, target_tlo_mm, message.clone());
            return Err(message);
        }
        if let Err(error) = send(Command::System(SystemCommand::Offsets)) {
            let message = format!(
                "{line} was sent but the `$#` read-back request failed: {error}; the TLO is \
                 unverified"
            );
            self.record_tool_length_failure(action, line, target_tlo_mm, message.clone());
            return Err(message);
        }
        let write = ToolLengthWrite {
            action,
            line,
            target_tlo_mm,
            status: ToolLengthWriteStatus::AwaitingReadBack,
            started_at: Instant::now(),
        };
        if let Some(run) = self.run.as_mut() {
            run.tool_length_confirm = None;
            run.tool_length_write = Some(write.clone());
        }
        Ok(write)
    }

    /// Verify the in-flight TLO write against the reported `$#` value.
    ///
    /// Returns `true` only when this report resolved the waiting write; an
    /// unrelated or already-resolved report changes nothing.
    pub fn on_offsets_reported(&mut self, tlo_z_mm: Option<f64>) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        let Some(write) = run.tool_length_write.as_mut() else {
            return false;
        };
        if !write.status.is_awaiting_read_back() {
            return false;
        }
        write.status = match tlo_z_mm {
            None => ToolLengthWriteStatus::Failed {
                message: "`$#` did not report a TLO value".to_string(),
            },
            Some(reported) if !reported.is_finite() => ToolLengthWriteStatus::Failed {
                message: "`$#` reported a non-finite TLO value".to_string(),
            },
            Some(reported)
                if (reported - write.target_tlo_mm).abs() <= TOOL_LENGTH_READBACK_TOLERANCE_MM =>
            {
                ToolLengthWriteStatus::Applied {
                    reported_tlo_mm: reported,
                }
            }
            Some(reported) => ToolLengthWriteStatus::Failed {
                message: format!(
                    "`$#` reported TLO {} mm but the target is {} mm",
                    format_mm(reported),
                    format_mm(write.target_tlo_mm)
                ),
            },
        };
        true
    }

    /// Mark a waiting TLO write as failed after an alarm, error, disconnect or
    /// operator timeout. A resolved write is left unchanged.
    ///
    /// Returns `true` when this call changed the state.
    pub fn fail_tool_length_write(&mut self, reason: impl Into<String>) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        let Some(write) = run.tool_length_write.as_mut() else {
            return false;
        };
        if !write.status.is_awaiting_read_back() {
            return false;
        }
        write.status = ToolLengthWriteStatus::Failed {
            message: reason.into(),
        };
        true
    }

    /// Fail a waiting TLO write once the `$#` deadline expires.
    ///
    /// `now` is supplied by the shell so the timeout is deterministic in tests.
    ///
    /// Returns `true` when this call timed the write out.
    pub fn check_tool_length_timeout(&mut self, now: Instant) -> bool {
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        let Some(write) = run.tool_length_write.as_mut() else {
            return false;
        };
        if !write.status.is_awaiting_read_back() {
            return false;
        }
        if now.saturating_duration_since(write.started_at) < TOOL_LENGTH_READBACK_TIMEOUT {
            return false;
        }
        write.status = ToolLengthWriteStatus::Failed {
            message: "timed out waiting for the `$#` TLO read-back".to_string(),
        };
        true
    }

    fn record_tool_length_failure(
        &mut self,
        action: ToolLengthAction,
        line: String,
        target_tlo_mm: f64,
        message: String,
    ) {
        if let Some(run) = self.run.as_mut() {
            run.tool_length_confirm = None;
            run.tool_length_write = Some(ToolLengthWrite {
                action,
                line,
                target_tlo_mm,
                status: ToolLengthWriteStatus::Failed { message },
                started_at: Instant::now(),
            });
        }
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
///
/// The captured datum shift is applied so a stopped run displays the same
/// effective heights as a completed one; raw samples stay raw.
fn partial_height_map(setup: &ProbeSetup, results: &[ProbeResult]) -> Option<HeightMap> {
    let ProbeTarget::HeightMap(grid) = &setup.target else {
        return None;
    };
    HeightMap::from_results(grid.clone(), results)
        .and_then(|map| map.with_z_offset(setup.map_z_offset_mm))
        .ok()
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
    /// Constant datum shift included in every effective height, in
    /// millimetres. Raw samples never include it.
    pub z_offset_mm: f64,
}

/// Summarise a height map without filling any missing cell.
///
/// Every reported Z and compensation is effective: the raw sample plus the
/// map's [`HeightMap::z_offset_mm`]. Raw samples remain available through
/// [`HeightMap::samples`] for persistence.
#[must_use]
pub fn height_map_summary(map: &HeightMap) -> HeightMapSummary {
    let (min_z, max_z) = map
        .z_range()
        .map_or((None, None), |(low, high)| (Some(low), Some(high)));
    let nominal = map.grid().surface_z_mm;
    let offset = map.z_offset_mm();
    let mut min_comp: Option<f64> = None;
    let mut max_comp: Option<f64> = None;
    for z in map.samples().iter().flatten() {
        let compensation = (z + offset) - nominal;
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
        z_offset_mm: offset,
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
        "column,row,machine_x_mm,machine_y_mm,effective_machine_z_mm,compensation_mm,valid\n",
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

/// Best-effort mapping from a rectangle/plate validation message to an editor
/// field, so a cross-field plan error is shown inline instead of only in the
/// preflight `PLAN` row.
fn cycle_error_field(message: &str) -> Option<ProbeField> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("rectangle width") || lower.contains("width_mm") {
        return Some(ProbeField::RectangleWidth);
    }
    if lower.contains("rectangle height") || lower.contains("height_mm") {
        return Some(ProbeField::RectangleHeight);
    }
    if lower.contains("diameter") {
        return Some(ProbeField::Diameter);
    }
    if lower.contains("probing height") {
        return Some(ProbeField::ProbingHeight);
    }
    if lower.contains("safe_z") || lower.contains("safe z") {
        return Some(ProbeField::SafeZ);
    }
    if lower.contains("edge_offset") {
        return Some(ProbeField::EdgeOffset);
    }
    if lower.contains("search") {
        return Some(ProbeField::Search);
    }
    if lower.contains("feed") {
        return Some(ProbeField::Feed);
    }
    if lower.contains("retract") {
        return Some(ProbeField::Retract);
    }
    None
}

/// TLO values implied by a completed tool-length run.
///
/// Returns `(current_tlo_mm, delta_mm)`.
fn tool_length_parts(run: &ProbeRun) -> Result<(f64, f64), String> {
    if run.phase != ProbePhase::Complete {
        return Err("the probe run has not completed".to_string());
    }
    let outcome = run
        .outcome
        .as_ref()
        .ok_or_else(|| "no computed probe value".to_string())?;
    let ProbeValue::ToolLength { offset_mm, .. } = &outcome.value else {
        return Err("this probe run is not a tool-length cycle".to_string());
    };
    Ok((run.setup.tlo_z_mm, *offset_mm))
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
            return Err("tool-length uses a G43.1 TLO write, not a G10 L2 WCS offset".to_string());
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
            mode,
        } => {
            let (cx, cy) = params
                .center
                .map(|center| (center.get(CoreAxis::X), center.get(CoreAxis::Y)))
                .unwrap_or((0.0, 0.0));
            format!(
                "center X{} Y{}, rectangle {} x {} mm, {}",
                format_mm(cx),
                format_mm(cy),
                format_mm(*width_mm),
                format_mm(*height_mm),
                mode.as_str()
            )
        }
        ProbeCycle::Rotation(rotation) => format!(
            "edge {} mm, separation {} mm, {} approach",
            format_mm(rotation.expected_edge_mm),
            format_mm(rotation.separation_mm),
            rotation.approach.as_str()
        ),
        ProbeCycle::XyTouchPlate(corner) => format!(
            "plate corner X {} mm ({}), Y {} mm ({}), outside",
            format_mm(corner.x.expected_edge_mm),
            corner.x.approach.as_str(),
            format_mm(corner.y.expected_edge_mm),
            corner.y.approach.as_str()
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

/// Round a millimetre value to the three decimals used by emitted lines.
fn round_mm(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

/// Format a millimetre value with three decimals, normalising negative zero.
pub fn format_mm(value: f64) -> String {
    let rounded = round_mm(value);
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
    use ghs_probe::{MAX_PROBE_MACRO_BYTES, MAX_PROBE_MACRO_LINES};

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

    fn recorded_send<'a>(
        sent: &'a mut Vec<Command>,
    ) -> impl FnMut(Command) -> Result<(), String> + 'a {
        move |command| {
            sent.push(command);
            Ok(())
        }
    }

    fn ready_tlo_gate() -> ToolLengthGate {
        ToolLengthGate {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            offsets_pending: false,
        }
    }

    /// Build a completed tool-length run with a known reference/contact pair.
    fn completed_tool_length(
        current_tlo_mm: f64,
        reference_z_mm: f64,
        contact_z_mm: f64,
    ) -> ProbeModel {
        let mut ctx = context();
        ctx.tlo_z_mm = current_tlo_mm;
        ctx.machine_pos_mm = Position::xyz(0.0, 0.0, 0.0);
        let mut model = ProbeModel::default();
        assert!(model.select_card(ProbeCard::ToolLength, &ctx));
        model.probe_connected_confirmed = true;
        model
            .set_field_text(ProbeField::ReferenceZ, format!("{reference_z_mm:.3}"))
            .expect("reference Z");
        let setup = model.prepare(&ctx).expect("tool-length plan");
        let name = setup.plan.name.clone();
        model.begin(setup);
        assert!(model.on_job_loaded(&name));
        assert!(model.on_job_started());
        assert!(model.bind_result(ProbeResult::new(
            true,
            Position::xyz(0.0, 0.0, contact_z_mm)
        )));
        assert!(model.on_job_completed());
        model
    }

    #[test]
    fn zero_probe_offset_keeps_the_previous_plan_and_result_exactly() {
        let ctx = context();
        let model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("plan");
        assert_eq!(setup.probe_offset, XyPoint::new(0.0, 0.0));
        assert_eq!(setup.map_z_offset_mm, 0.0);

        let ProbeTarget::Cycle(cycle) = &setup.target else {
            panic!("wrong target");
        };
        let direct = cycle.plan(&setup.params).expect("zero-offset plan");
        assert_eq!(setup.plan.program, direct.program);
        assert_eq!(setup.plan.probes, direct.probes);
        assert_eq!(setup.plan.expected_probes, direct.expected_probes);

        let contact = Position::xyz(6.0, 8.0, -11.0);
        let direct_outcome =
            ProbeOutcome::from_results(cycle, &setup.params, &[ProbeResult::new(true, contact)])
                .expect("zero-offset result");
        let mut run_model = model;
        run_model.begin(setup);
        assert!(run_model.on_job_loaded("z-touch-plate"));
        assert!(run_model.on_job_started());
        assert!(run_model.bind_result(ProbeResult::new(true, contact)));
        assert!(run_model.on_job_completed());
        assert_eq!(
            run_model.run().and_then(|run| run.outcome.clone()),
            Some(direct_outcome)
        );
    }

    #[test]
    fn nonzero_probe_offset_shifts_the_plan_and_binds_feature_coordinates() {
        let ctx = context();
        let mut model = model(ProbeCard::EdgeX);
        model
            .set_probe_offset(XyPoint::new(10.0, -5.0))
            .expect("offset");
        let setup = model.prepare(&ctx).expect("offset plan");
        assert_eq!(setup.probe_offset, XyPoint::new(10.0, -5.0));
        assert_eq!(setup.plan.expected_probes, 1);

        // Expected feature edge 20 mm, radius 1.5, outside +X: feature hint
        // 18.5, feature start 13.5. Controlled = feature - (10, -5).
        let probe = &setup.plan.probes[0];
        assert!((probe.from.get(CoreAxis::X) - 3.5).abs() < 1.0e-9);
        // Feature Y is 20 mm; subtracting a -5 mm offset commands 25 mm.
        assert!(
            (probe.from.get(CoreAxis::Y) - 25.0).abs() < 1.0e-9,
            "from {:?} to {:?}",
            probe.from,
            probe.to
        );
        assert!((probe.to.get(CoreAxis::X) - 8.5).abs() < 1.0e-9);
        assert!((probe.to.get(CoreAxis::Y) - 25.0).abs() < 1.0e-9);
        // The program is in work coordinates: controlled X 8.5 - WCS X 6.
        assert!(
            setup.plan.program.contains("G38.2 X2.500"),
            "{}",
            setup.plan.program
        );

        model.begin(setup);
        assert!(model.on_job_loaded("edge-x-outside-positive"));
        assert!(model.on_job_started());
        // The raw PRB stays at the controlled point (8.5, 5). The outcome
        // adds the offset back: feature contact X 18.5, edge 20.
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(8.5, 25.0, 0.0))));
        assert!(model.on_job_completed());
        let run = model.run().expect("run");
        assert_eq!(run.results[0].position.get(CoreAxis::X), 8.5);
        let ProbeValue::Edge {
            edge_mm, zero_mm, ..
        } = &run.outcome.as_ref().expect("outcome").value
        else {
            panic!("wrong value");
        };
        assert!((*edge_mm - 20.0).abs() < 1.0e-9, "{edge_mm}");
        assert!((*zero_mm - 20.0).abs() < 1.0e-9, "{zero_mm}");
    }

    #[test]
    fn invalid_probe_offset_is_refused_with_a_clear_error() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        let error = model
            .set_field_text(ProbeField::ProbeOffsetX, "200.000")
            .expect_err("out of range");
        assert!(error.contains("PROBE OFFSET X"), "{error}");
        assert!(error.contains("100.000"), "{error}");
        assert_eq!(model.field_text(ProbeField::ProbeOffsetX), "0.000");
        assert!(
            model
                .field_error(ProbeField::ProbeOffsetX)
                .is_some_and(|text| text.contains("PROBE OFFSET X"))
        );
        assert!(
            model.prepare(&ctx).is_ok(),
            "the old zero offset still runs"
        );

        let error = model
            .set_probe_offset(XyPoint::new(f64::NAN, 0.0))
            .expect_err("non-finite");
        assert!(error.to_lowercase().contains("finite"), "{error}");
        assert_eq!(
            model.probe_offset().expect("still zero"),
            XyPoint::new(0.0, 0.0)
        );
    }

    #[test]
    fn map_z_offset_is_effective_in_summary_overlay_and_csv() {
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(10.0, 20.0),
            step_x_mm: 4.0,
            step_y_mm: 6.0,
            columns: 2,
            rows: 2,
            surface_z_mm: 0.0,
        };
        let map = HeightMap::from_samples(grid, vec![Some(1.0), None, Some(3.0), Some(4.0)])
            .expect("map")
            .with_z_offset(0.75)
            .expect("offset");
        assert_eq!(map.samples()[0], Some(1.0), "raw samples stay raw");

        let summary = height_map_summary(&map);
        assert_eq!(summary.z_offset_mm, 0.75);
        assert_eq!(summary.min_z_mm, Some(1.75));
        assert_eq!(summary.max_z_mm, Some(4.75));
        assert_eq!(summary.min_compensation_mm, Some(1.75));
        assert_eq!(summary.max_compensation_mm, Some(4.75));

        let overlay = height_map_overlay(&map, Position::xyz(5.0, 6.0, 3.0));
        assert_eq!(overlay.cells[0].z_mm, Some(1.75));
        assert_eq!(overlay.cells[1].z_mm, None);
        assert_eq!(overlay.cells[3].z_mm, Some(4.75));

        let csv = height_map_csv(&map);
        assert!(csv.contains("0,0,10.000,20.000,1.750,1.750,true"), "{csv}");
        assert!(csv.contains("1,1,14.000,26.000,4.750,4.750,true"), "{csv}");
    }

    #[test]
    fn height_map_run_captures_the_z_offset_without_changing_the_plan() {
        let ctx = context();
        let mut shifted = model(ProbeCard::HeightMap);
        set_height_grid(&mut shifted, 2, 2);
        shifted
            .set_field_text(ProbeField::MapZOffset, "1.250")
            .expect("z offset");
        let shifted_setup = shifted.prepare(&ctx).expect("shifted plan");

        let mut plain = model(ProbeCard::HeightMap);
        set_height_grid(&mut plain, 2, 2);
        let plain_setup = plain.prepare(&ctx).expect("plain plan");
        assert_eq!(
            shifted_setup.plan.program, plain_setup.plan.program,
            "the datum shift must not enter the plan"
        );
        assert_eq!(shifted_setup.plan.probes, plain_setup.plan.probes);
        assert_eq!(shifted_setup.map_z_offset_mm, 1.25);

        shifted.begin(shifted_setup);
        assert!(shifted.on_job_loaded("height-map"));
        assert!(shifted.on_job_started());
        for (x, z) in [(6.0, 0.0), (16.0, -1.0), (6.0, -2.0), (16.0, -3.0)] {
            let y = if x == 6.0 { 8.0 } else { 18.0 };
            assert!(shifted.bind_result(ProbeResult::new(true, Position::xyz(x, y, z))));
        }
        assert!(shifted.on_job_completed());
        let map = shifted.run().and_then(|run| run.map.as_ref()).expect("map");
        assert_eq!(map.z_offset_mm(), 1.25);
        assert_eq!(map.samples()[0], Some(0.0), "raw sample");
        assert_eq!(map.z(0, 0), Some(1.25), "effective value");
        assert_eq!(height_map_summary(map).min_z_mm, Some(-3.0 + 1.25));
    }

    #[test]
    fn preflight_shows_the_probe_offset_and_coordinate_convention() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        model
            .set_probe_offset(XyPoint::new(12.5, -8.0))
            .expect("offset");
        let sheet = model.preflight(&ctx);
        assert!(sheet.blockers.is_empty(), "{:?}", sheet.blockers);
        let offset = sheet
            .rows
            .iter()
            .find(|row| row.label == "PROBE OFFSET")
            .expect("offset row");
        assert!(offset.value.contains("X12.500"), "{}", offset.value);
        assert!(offset.value.contains("Y-8.000"), "{}", offset.value);
        let coordinates = sheet
            .rows
            .iter()
            .find(|row| row.label == "COORDINATES")
            .expect("coordinate row");
        assert!(coordinates.value.contains("feature"));
        assert!(coordinates.value.contains("controlled point"));
        assert!(coordinates.value.contains("feature - offset"));
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
            "column,row,machine_x_mm,machine_y_mm,effective_machine_z_mm,compensation_mm,valid"
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
    fn new_probe_cards_have_fields_and_build_the_expected_plans() {
        assert!(ProbeCard::ALL.contains(&ProbeCard::CenterRectangle));
        assert!(ProbeCard::ALL.contains(&ProbeCard::XyTouchPlate));
        assert_eq!(ProbeCard::ALL.len(), 10);

        let ctx = context();
        let rectangle = model(ProbeCard::CenterRectangle);
        assert_eq!(
            ProbeCard::CenterRectangle.fields(),
            &[
                ProbeField::CenterX,
                ProbeField::CenterY,
                ProbeField::ProbingHeight,
                ProbeField::RectangleWidth,
                ProbeField::RectangleHeight,
                ProbeField::Diameter,
                ProbeField::EdgeOffset,
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
            ]
        );
        let setup = rectangle.prepare(&ctx).expect("rectangle plan");
        assert_eq!(setup.plan.kind, ghs_probe::CycleKind::CenterRectangle);
        assert_eq!(setup.plan.name, "center-rectangle-inside");
        assert_eq!(setup.plan.expected_probes, 4);
        assert_eq!(
            setup
                .plan
                .probes
                .iter()
                .map(|probe| probe.label.as_str())
                .collect::<Vec<_>>(),
            vec!["x-plus", "x-minus", "y-plus", "y-minus"]
        );
        let ProbeTarget::Cycle(ProbeCycle::CenterRectangle {
            width_mm,
            height_mm,
            mode,
        }) = &setup.target
        else {
            panic!("wrong rectangle target");
        };
        assert_eq!(
            (*width_mm, *height_mm, *mode),
            (60.0, 40.0, EdgeMode::Inside)
        );

        let plate = model(ProbeCard::XyTouchPlate);
        assert_eq!(
            ProbeCard::XyTouchPlate.fields(),
            &[
                ProbeField::PlateX,
                ProbeField::PlateY,
                ProbeField::ProbingHeight,
                ProbeField::EdgeOffset,
                ProbeField::Diameter,
                ProbeField::SafeZ,
                ProbeField::Search,
                ProbeField::Feed,
                ProbeField::Retract,
            ]
        );
        let setup = plate.prepare(&ctx).expect("plate plan");
        assert_eq!(setup.plan.kind, ghs_probe::CycleKind::XyTouchPlate);
        assert_eq!(setup.plan.name, "xy-touch-plate");
        assert_eq!(setup.plan.expected_probes, 2);
        assert_eq!(
            setup
                .plan
                .probes
                .iter()
                .map(|probe| probe.label.as_str())
                .collect::<Vec<_>>(),
            vec!["x-edge", "y-edge"]
        );
        let ProbeTarget::Cycle(ProbeCycle::XyTouchPlate(corner)) = &setup.target else {
            panic!("wrong plate target");
        };
        assert_eq!(corner.x.axis, ProbeAxis::X);
        assert_eq!(corner.y.axis, ProbeAxis::Y);
        assert_eq!(corner.x.mode, EdgeMode::Outside);
        assert_eq!(corner.y.mode, EdgeMode::Outside);
        assert_eq!(corner.x.approach, Sign::Positive);
        assert_eq!(corner.y.approach, Sign::Positive);
        // Corner (20,20) plus one radius + search inboard = anchor (26.5, 26.5).
        assert_eq!(setup.params.position, Some(Position::xyz(26.5, 26.5, 0.0)));
    }

    #[test]
    fn rectangle_results_report_center_size_and_de_biased_g10() {
        let ctx = context();
        let mut model = model(ProbeCard::CenterRectangle);
        for (field, text) in [
            (ProbeField::CenterX, "10.000"),
            (ProbeField::CenterY, "20.000"),
            (ProbeField::ProbingHeight, "0.000"),
            (ProbeField::Diameter, "2.000"),
            (ProbeField::RectangleWidth, "20.000"),
            (ProbeField::RectangleHeight, "10.000"),
        ] {
            model.set_field_text(field, text).expect("field");
        }
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("center-rectangle-inside"));
        assert!(model.on_job_started());
        // Inside a 20 x 10 rectangle centred at (10,20): x edges 0/20, y 15/25.
        for position in [
            Position::xyz(19.0, 20.0, 0.0),
            Position::xyz(1.0, 20.0, 0.0),
            Position::xyz(10.0, 24.0, 0.0),
            Position::xyz(10.0, 16.0, 0.0),
        ] {
            assert!(model.bind_result(ProbeResult::new(true, position)));
        }
        assert!(model.on_job_completed());
        let outcome = model
            .run()
            .and_then(|run| run.outcome.as_ref())
            .expect("outcome");
        let ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } = &outcome.value
        else {
            panic!("wrong value");
        };
        assert!((center.x_mm - 10.0).abs() < 1.0e-9);
        assert!((center.y_mm - 20.0).abs() < 1.0e-9);
        assert!((*width_mm - 20.0).abs() < 1.0e-9);
        assert!((*height_mm - 10.0).abs() < 1.0e-9);

        let summary = outcome_summary(outcome);
        assert_eq!(summary.headline, "CENTER 10.000 20.000");
        assert!(
            summary
                .details
                .iter()
                .any(|(label, value)| label == "Measured width" && value == "20.000 mm")
        );
        assert!(
            summary
                .details
                .iter()
                .any(|(label, value)| label == "Measured height" && value == "10.000 mm")
        );
        // G92 X1/Y2 is preserved and removed from the stored WCS write.
        assert_eq!(
            model.apply_lines().expect("apply"),
            vec!["G10 L2 P1 X9.000 Y18.000".to_string()]
        );
    }

    #[test]
    fn xy_plate_results_report_edge_zero_and_de_biased_g10() {
        let ctx = context();
        let mut model = model(ProbeCard::XyTouchPlate);
        for (field, text) in [
            (ProbeField::PlateX, "20.000"),
            (ProbeField::PlateY, "30.000"),
            (ProbeField::ProbingHeight, "0.000"),
            (ProbeField::Diameter, "3.000"),
            (ProbeField::EdgeOffset, "2.000"),
        ] {
            model.set_field_text(field, text).expect("field");
        }
        let setup = model.prepare(&ctx).expect("plan");
        model.begin(setup);
        assert!(model.on_job_loaded("xy-touch-plate"));
        assert!(model.on_job_started());
        // Outside +X/+Y with radius 1.5: contacts 18.5/28.5 give edges 20/30.
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(18.5, 0.0, 0.0))));
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(0.0, 28.5, 0.0))));
        assert!(model.on_job_completed());
        let outcome = model
            .run()
            .and_then(|run| run.outcome.as_ref())
            .expect("outcome");
        let ProbeValue::XyZero {
            x_edge_mm,
            y_edge_mm,
            zero,
        } = &outcome.value
        else {
            panic!("wrong value");
        };
        assert!((*x_edge_mm - 20.0).abs() < 1.0e-9);
        assert!((*y_edge_mm - 30.0).abs() < 1.0e-9);
        assert_eq!(*zero, XyPoint::new(22.0, 32.0));

        let summary = outcome_summary(outcome);
        assert_eq!(summary.headline, "WORK X0 Y0 22.000 32.000");
        assert!(
            summary
                .details
                .iter()
                .any(|(label, value)| label == "Measured X edge" && value == "20.000 mm")
        );
        assert!(
            summary
                .details
                .iter()
                .any(|(label, value)| label == "Measured Y edge" && value == "30.000 mm")
        );
        // G92 X1/Y2 is preserved and removed from the stored WCS write.
        assert_eq!(
            model.apply_lines().expect("apply"),
            vec!["G10 L2 P1 X21.000 Y30.000".to_string()]
        );
    }

    #[test]
    fn rectangle_dimensions_radius_and_plate_position_are_refused() {
        let ctx = context();
        let mut rectangle = model(ProbeCard::CenterRectangle);
        rectangle
            .set_field_text(ProbeField::RectangleWidth, "0.000")
            .expect("zero is within the editor bound");
        let error = rectangle.prepare(&ctx).expect_err("zero width");
        assert!(error.contains("width_mm"), "{error}");
        assert!(rectangle.field_problems(&ctx).iter().any(|problem| {
            problem.field == Some(ProbeField::RectangleWidth)
                && problem.message.contains("width_mm")
        }));

        rectangle
            .set_field_text(ProbeField::RectangleWidth, "20.000")
            .expect("width");
        rectangle
            .set_field_text(ProbeField::RectangleHeight, "0.000")
            .expect("zero is within the editor bound");
        let error = rectangle.prepare(&ctx).expect_err("zero height");
        assert!(error.contains("height_mm"), "{error}");
        assert!(rectangle.field_problems(&ctx).iter().any(|problem| {
            problem.field == Some(ProbeField::RectangleHeight)
                && problem.message.contains("height_mm")
        }));

        rectangle
            .set_field_text(ProbeField::RectangleHeight, "10.000")
            .expect("height");
        rectangle
            .set_field_text(ProbeField::Diameter, "50.000")
            .expect("diameter");
        let error = rectangle.prepare(&ctx).expect_err("probe too large");
        assert!(error.contains("rectangle width"), "{error}");
        assert!(rectangle.field_problems(&ctx).iter().any(|problem| {
            problem.field == Some(ProbeField::RectangleWidth)
                && problem.message.contains("rectangle width")
        }));

        let mut plate = model(ProbeCard::XyTouchPlate);
        plate
            .set_field_text(ProbeField::ProbingHeight, "20.000")
            .expect("height");
        let error = plate.prepare(&ctx).expect_err("height above safe Z");
        assert!(error.contains("probing height"), "{error}");
        assert!(plate.field_problems(&ctx).iter().any(|problem| {
            problem.field == Some(ProbeField::ProbingHeight)
                && problem.message.contains("probing height")
        }));
        let error = plate
            .set_field_text(ProbeField::PlateX, "100001.000")
            .expect_err("coordinate bound");
        assert!(error.contains("PLATE CORNER X"), "{error}");
        assert!(
            plate
                .field_error(ProbeField::PlateX)
                .is_some_and(|message| message.contains("PLATE CORNER X"))
        );
    }

    #[test]
    fn probe_offset_shifts_both_new_plans_and_results() {
        let ctx = context();
        let offset = XyPoint::new(10.0, -5.0);

        let mut rectangle = model(ProbeCard::CenterRectangle);
        for (field, text) in [
            (ProbeField::CenterX, "10.000"),
            (ProbeField::CenterY, "20.000"),
            (ProbeField::ProbingHeight, "0.000"),
            (ProbeField::Diameter, "2.000"),
            (ProbeField::RectangleWidth, "20.000"),
            (ProbeField::RectangleHeight, "10.000"),
        ] {
            rectangle.set_field_text(field, text).expect("field");
        }
        rectangle.set_probe_offset(offset).expect("offset");
        let zero_setup = {
            let mut plain = rectangle.clone();
            plain
                .set_probe_offset(XyPoint::new(0.0, 0.0))
                .expect("zero");
            plain.prepare(&ctx).expect("zero plan")
        };
        let setup = rectangle.prepare(&ctx).expect("offset plan");
        assert_eq!(setup.probe_offset, offset);
        assert_eq!(setup.plan.expected_probes, 4);
        assert_shifted_plan(&zero_setup.plan, &setup.plan, offset);
        rectangle.begin(setup);
        assert!(rectangle.on_job_loaded("center-rectangle-inside"));
        assert!(rectangle.on_job_started());
        for position in [
            Position::xyz(9.0, 25.0, 0.0),
            Position::xyz(-9.0, 25.0, 0.0),
            Position::xyz(0.0, 29.0, 0.0),
            Position::xyz(0.0, 21.0, 0.0),
        ] {
            assert!(rectangle.bind_result(ProbeResult::new(true, position)));
        }
        assert!(rectangle.on_job_completed());
        let ProbeValue::RectangleCenter {
            center,
            width_mm,
            height_mm,
        } = &rectangle
            .run()
            .and_then(|run| run.outcome.as_ref())
            .expect("outcome")
            .value
        else {
            panic!("wrong value");
        };
        assert!((center.x_mm - 10.0).abs() < 1.0e-9, "{center:?}");
        assert!((center.y_mm - 20.0).abs() < 1.0e-9, "{center:?}");
        assert!((*width_mm - 20.0).abs() < 1.0e-9);
        assert!((*height_mm - 10.0).abs() < 1.0e-9);
        assert_eq!(
            rectangle.apply_lines().expect("apply"),
            vec!["G10 L2 P1 X9.000 Y18.000".to_string()]
        );

        let mut plate = model(ProbeCard::XyTouchPlate);
        for (field, text) in [
            (ProbeField::PlateX, "20.000"),
            (ProbeField::PlateY, "30.000"),
            (ProbeField::ProbingHeight, "0.000"),
            (ProbeField::Diameter, "3.000"),
        ] {
            plate.set_field_text(field, text).expect("field");
        }
        plate.set_probe_offset(offset).expect("offset");
        let zero_setup = {
            let mut plain = plate.clone();
            plain
                .set_probe_offset(XyPoint::new(0.0, 0.0))
                .expect("zero");
            plain.prepare(&ctx).expect("zero plan")
        };
        let setup = plate.prepare(&ctx).expect("offset plan");
        assert_eq!(setup.probe_offset, offset);
        assert_shifted_plan(&zero_setup.plan, &setup.plan, offset);
        plate.begin(setup);
        assert!(plate.on_job_loaded("xy-touch-plate"));
        assert!(plate.on_job_started());
        assert!(plate.bind_result(ProbeResult::new(true, Position::xyz(8.5, 0.0, 0.0))));
        assert!(plate.bind_result(ProbeResult::new(true, Position::xyz(0.0, 33.5, 0.0))));
        assert!(plate.on_job_completed());
        let ProbeValue::XyZero { zero, .. } = &plate
            .run()
            .and_then(|run| run.outcome.as_ref())
            .expect("outcome")
            .value
        else {
            panic!("wrong value");
        };
        assert!((zero.x_mm - 20.0).abs() < 1.0e-9, "{zero:?}");
        assert!((zero.y_mm - 30.0).abs() < 1.0e-9, "{zero:?}");
        assert_eq!(
            plate.apply_lines().expect("apply"),
            vec!["G10 L2 P1 X19.000 Y28.000".to_string()]
        );
    }

    #[test]
    fn result_binding_order_is_four_for_rectangles_and_two_for_plates() {
        let ctx = context();
        let mut rectangle = model(ProbeCard::CenterRectangle);
        for (field, text) in [
            (ProbeField::CenterX, "10.000"),
            (ProbeField::CenterY, "20.000"),
            (ProbeField::Diameter, "2.000"),
            (ProbeField::RectangleWidth, "20.000"),
            (ProbeField::RectangleHeight, "10.000"),
        ] {
            rectangle.set_field_text(field, text).expect("field");
        }
        let setup = rectangle.prepare(&ctx).expect("plan");
        rectangle.begin(setup);
        assert!(rectangle.on_job_loaded("center-rectangle-inside"));
        assert!(rectangle.on_job_started());
        for (index, position) in [
            Position::xyz(19.0, 20.0, 0.0),
            Position::xyz(1.0, 20.0, 0.0),
            Position::xyz(10.0, 24.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(rectangle.bind_result(ProbeResult::new(true, position)));
            assert_eq!(rectangle.run().expect("run").bound(), index + 1);
        }
        assert!(rectangle.bind_result(ProbeResult::new(true, Position::xyz(10.0, 16.0, 0.0))));
        assert!(rectangle.on_job_completed());
        assert_eq!(rectangle.run().expect("run").bound(), 4);
        assert!(
            !rectangle.bind_result(ProbeResult::new(true, Position::xyz(0.0, 0.0, 0.0))),
            "a completed run accepts no further result"
        );

        let mut plate = model(ProbeCard::XyTouchPlate);
        let setup = plate.prepare(&ctx).expect("plan");
        plate.begin(setup);
        assert!(plate.on_job_loaded("xy-touch-plate"));
        assert!(plate.on_job_started());
        assert!(plate.bind_result(ProbeResult::new(true, Position::xyz(18.5, 0.0, 0.0))));
        assert_eq!(plate.run().expect("run").bound(), 1);
        assert!(plate.bind_result(ProbeResult::new(true, Position::xyz(0.0, 28.5, 0.0))));
        assert_eq!(plate.run().expect("run").bound(), 2);
        assert!(plate.on_job_completed());
        assert!(
            !plate.bind_result(ProbeResult::new(true, Position::xyz(0.0, 0.0, 0.0))),
            "a completed plate run accepts no further result"
        );
    }

    #[test]
    fn zero_probe_offset_keeps_prior_new_cycle_behavior() {
        let ctx = context();
        for (card, positions) in [
            (
                ProbeCard::CenterRectangle,
                vec![
                    Position::xyz(28.5, 20.0, 0.0),
                    Position::xyz(-28.5, 20.0, 0.0),
                    Position::xyz(0.0, 18.5, 0.0),
                    Position::xyz(0.0, -18.5, 0.0),
                ],
            ),
            (
                ProbeCard::XyTouchPlate,
                vec![Position::xyz(18.5, 0.0, 0.0), Position::xyz(0.0, 18.5, 0.0)],
            ),
        ] {
            let model = model(card);
            let setup = model.prepare(&ctx).expect("plan");
            let ProbeTarget::Cycle(cycle) = &setup.target else {
                panic!("wrong target");
            };
            let cycle = *cycle;
            assert_eq!(setup.probe_offset, XyPoint::new(0.0, 0.0));
            assert_eq!(
                setup.plan,
                cycle.plan(&setup.params).expect("direct zero plan")
            );
            let direct = ProbeOutcome::from_results(
                &cycle,
                &setup.params,
                &positions
                    .iter()
                    .map(|position| ProbeResult::new(true, *position))
                    .collect::<Vec<_>>(),
            )
            .expect("direct zero outcome");

            let mut model = model;
            model.begin(setup);
            let name = cycle.name();
            assert!(model.on_job_loaded(&name));
            assert!(model.on_job_started());
            for position in positions {
                assert!(model.bind_result(ProbeResult::new(true, position)));
            }
            assert!(model.on_job_completed());
            assert_eq!(
                model.run().and_then(|run| run.outcome.clone()),
                Some(direct)
            );
        }
    }

    #[test]
    fn a_failed_new_cycle_result_or_alarm_stops_and_never_resumes() {
        let ctx = context();
        let mut rectangle = model(ProbeCard::CenterRectangle);
        let setup = rectangle.prepare(&ctx).expect("plan");
        rectangle.begin(setup);
        assert!(rectangle.on_job_loaded("center-rectangle-inside"));
        assert!(rectangle.on_job_started());
        assert!(rectangle.bind_result(ProbeResult::new(true, Position::xyz(29.0, 0.0, 0.0))));
        assert!(rectangle.on_alarm(5));
        let run = rectangle.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Failed);
        assert!(
            run.error
                .as_deref()
                .is_some_and(|text| text.contains("ALARM:5"))
        );
        assert!(!rectangle.needs_resume());
        assert!(!rectangle.apply_supported());
        assert!(rectangle.apply_lines().is_err());
        assert!(
            !rectangle.bind_result(ProbeResult::new(true, Position::xyz(0.0, 0.0, 0.0))),
            "an alarmed run accepts no further result"
        );

        let mut plate = model(ProbeCard::XyTouchPlate);
        let setup = plate.prepare(&ctx).expect("plan");
        plate.begin(setup);
        assert!(plate.on_job_loaded("xy-touch-plate"));
        assert!(plate.on_job_started());
        assert!(plate.bind_result(ProbeResult::new(true, Position::xyz(18.5, 0.0, 0.0))));
        assert!(plate.bind_result(ProbeResult::new(false, Position::xyz(0.0, 5.0, 0.0))));
        let run = plate.run().expect("run");
        assert_eq!(run.phase, ProbePhase::Failed);
        assert!(
            run.error
                .as_deref()
                .is_some_and(|text| text.contains("reported failure"))
        );
        assert!(plate.take_stop_request(), "the shell must stop the job");
        assert!(
            !plate.take_stop_request(),
            "the stop request is consumed once"
        );
        assert!(!plate.needs_resume());
        assert!(!plate.apply_supported());
        assert!(
            !plate.bind_result(ProbeResult::new(true, Position::xyz(0.0, 28.5, 0.0))),
            "a failed plate run accepts no further result"
        );
    }

    fn assert_shifted_plan(zero: &ProbePlan, shifted: &ProbePlan, offset: XyPoint) {
        assert_eq!(zero.probes.len(), shifted.probes.len());
        for (zero_probe, shifted_probe) in zero.probes.iter().zip(&shifted.probes) {
            for (zero_position, shifted_position) in [
                (zero_probe.from, shifted_probe.from),
                (zero_probe.to, shifted_probe.to),
            ] {
                assert!(
                    (shifted_position.get(CoreAxis::X)
                        - (zero_position.get(CoreAxis::X) - offset.x_mm))
                        .abs()
                        < 1.0e-9
                );
                assert!(
                    (shifted_position.get(CoreAxis::Y)
                        - (zero_position.get(CoreAxis::Y) - offset.y_mm))
                        .abs()
                        < 1.0e-9
                );
            }
        }
    }

    #[test]
    fn tool_length_preview_adds_a_positive_delta_to_the_current_tlo() {
        let model = completed_tool_length(4.0, -10.0, -8.0);
        let preview = model.tool_length_preview().expect("preview");
        assert_eq!(preview.current_tlo_mm, 4.0);
        assert_eq!(preview.delta_mm, 2.0);
        assert_eq!(preview.target_tlo_mm, 6.0);
        assert_eq!(preview.apply_line(), "G43.1 Z6.000");
    }

    #[test]
    fn tool_length_preview_adds_a_negative_delta_to_the_current_tlo() {
        let model = completed_tool_length(4.0, -10.0, -13.0);
        let preview = model.tool_length_preview().expect("preview");
        assert_eq!(preview.current_tlo_mm, 4.0);
        assert_eq!(preview.delta_mm, -3.0);
        assert_eq!(preview.target_tlo_mm, 1.0);
        assert_eq!(preview.apply_line(), "G43.1 Z1.000");
    }

    #[test]
    fn tool_length_builds_exactly_g431_and_g49() {
        let model = completed_tool_length(4.0, -10.0, -8.0);
        let preview = model.tool_length_preview().expect("preview");
        assert_eq!(preview.apply_line(), "G43.1 Z6.000");
        assert_eq!(
            model.tool_length_clear_line().expect("clear line"),
            "G49".to_string()
        );
        assert!(!preview.apply_line().contains('$'));
        assert!(!preview.apply_line().contains("TLR"));
    }

    #[test]
    fn tool_length_requires_confirmation_and_cancel_sends_nothing() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        let mut sent = Vec::new();
        assert!(
            model
                .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
                .is_err(),
            "no confirmation was opened"
        );
        assert!(sent.is_empty(), "confirmation is mandatory");

        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        assert_eq!(
            model.tool_length_confirmation(),
            Some(ToolLengthAction::Apply)
        );
        assert!(sent.is_empty(), "opening the sheet sends nothing");

        assert!(model.cancel_tool_length_action());
        assert_eq!(model.tool_length_confirmation(), None);
        assert!(
            model
                .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
                .is_err(),
            "cancel must not leave a command behind"
        );
        assert!(sent.is_empty(), "cancel sends nothing");
    }

    #[test]
    fn confirming_tool_length_sends_one_mdi_line_and_one_offsets_request() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        let write = model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed write");
        assert_eq!(
            sent,
            vec![
                Command::Mdi("G43.1 Z6.000".to_string()),
                Command::System(SystemCommand::Offsets),
            ]
        );
        assert_eq!(write.line, "G43.1 Z6.000");
        assert_eq!(write.target_tlo_mm, 6.0);
        assert_eq!(write.status, ToolLengthWriteStatus::AwaitingReadBack);
        assert!(model.tool_length_write_in_flight());
    }

    #[test]
    fn tool_length_gates_refuse_before_sending_anything() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let gates = [
            (
                ToolLengthGate {
                    live: false,
                    ..ready_tlo_gate()
                },
                "live controller",
            ),
            (
                ToolLengthGate {
                    ready: false,
                    ..ready_tlo_gate()
                },
                "not ready",
            ),
            (
                ToolLengthGate {
                    machine_state: MachineState::Alarm,
                    ..ready_tlo_gate()
                },
                "Alarm",
            ),
            (
                ToolLengthGate {
                    machine_state: MachineState::Door,
                    ..ready_tlo_gate()
                },
                "Door",
            ),
            (
                ToolLengthGate {
                    job_active: true,
                    ..ready_tlo_gate()
                },
                "job is active",
            ),
            (
                ToolLengthGate {
                    machine_state: MachineState::Hold,
                    ..ready_tlo_gate()
                },
                "Idle",
            ),
            (
                ToolLengthGate {
                    offsets_pending: true,
                    ..ready_tlo_gate()
                },
                "$#",
            ),
        ];
        for (gate, needle) in gates {
            let mut sent = Vec::new();
            let error = model
                .confirm_tool_length_action(gate, recorded_send(&mut sent))
                .expect_err("closed gate");
            assert!(error.contains(needle), "{error}");
            assert!(sent.is_empty(), "a refused gate sends nothing");
        }

        // A still-open confirmation works once the gate is open again.
        let mut sent = Vec::new();
        model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("gate reopened");
        assert_eq!(sent.len(), 2);
    }

    #[test]
    fn matching_tool_length_read_back_marks_applied() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed write");

        assert!(model.on_offsets_reported(Some(6.0)));
        assert_eq!(
            model.tool_length_write().map(|write| &write.status),
            Some(&ToolLengthWriteStatus::Applied {
                reported_tlo_mm: 6.0
            })
        );
        let preview = model.tool_length_preview().expect("old preview retained");
        assert_eq!(preview.current_tlo_mm, 4.0);
        assert_eq!(preview.target_tlo_mm, 6.0);
    }

    #[test]
    fn mismatched_tool_length_read_back_fails_and_keeps_the_old_value() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed write");

        assert!(model.on_offsets_reported(Some(6.5)));
        let write = model.tool_length_write().expect("write");
        assert!(
            matches!(
                &write.status,
                ToolLengthWriteStatus::Failed { message }
                    if message.contains("6.500") && message.contains("6.000")
            ),
            "{:?}",
            write.status
        );
        let preview = model.tool_length_preview().expect("preview");
        assert_eq!(preview.current_tlo_mm, 4.0, "old TLO stays displayed");
        assert_eq!(preview.target_tlo_mm, 6.0);
    }

    #[test]
    fn non_finite_or_out_of_range_tool_length_targets_are_refused() {
        let mut out_of_range = completed_tool_length(MAX_TOOL_LENGTH_MM, -10.0, -8.0);
        let error = out_of_range
            .tool_length_preview()
            .expect_err("target above the bound");
        assert!(error.contains("outside"), "{error}");
        assert!(
            out_of_range
                .request_tool_length_action(ToolLengthAction::Apply)
                .is_err()
        );
        assert_eq!(out_of_range.tool_length_confirmation(), None);

        let mut non_finite = completed_tool_length(4.0, -10.0, -8.0);
        if let Some(run) = non_finite.run_mut() {
            run.setup.tlo_z_mm = f64::NAN;
        }
        let error = non_finite
            .tool_length_preview()
            .expect_err("non-finite current TLO");
        assert!(error.contains("current TLO"), "{error}");

        let mut bad_delta = completed_tool_length(4.0, -10.0, -8.0);
        if let Some(outcome) = bad_delta.run_mut().and_then(|run| run.outcome.as_mut()) {
            let ProbeValue::ToolLength { offset_mm, .. } = &mut outcome.value else {
                panic!("wrong value");
            };
            *offset_mm = f64::INFINITY;
        }
        let error = bad_delta
            .tool_length_preview()
            .expect_err("non-finite delta");
        assert!(error.contains("delta"), "{error}");
    }

    #[test]
    fn tool_length_write_never_emits_motion_start_load_g10_or_tlr() {
        for action in [ToolLengthAction::Apply, ToolLengthAction::Clear] {
            let mut model = completed_tool_length(4.0, -10.0, -8.0);
            model
                .request_tool_length_action(action)
                .expect("open confirmation");
            let mut sent = Vec::new();
            model
                .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
                .expect("confirmed write");
            assert_eq!(sent.len(), 2);
            for command in &sent {
                match command {
                    Command::Mdi(line) => {
                        assert!(
                            line == "G43.1 Z6.000" || line == "G49",
                            "unexpected MDI line {line}"
                        );
                        for forbidden in [
                            "G0", "G1 ", "G2 ", "G3 ", "G10", "G28", "G30", "G53", "G92", "M3",
                            "M4", "M5", "M6", "M7", "M8", "M9", "$TLR",
                        ] {
                            assert!(!line.contains(forbidden), "forbidden {forbidden} in {line}");
                        }
                    }
                    Command::System(SystemCommand::Offsets) => {}
                    other => panic!("unexpected command {other:?}"),
                }
            }
            assert!(
                !sent.iter().any(|command| matches!(
                    command,
                    Command::LoadProgram { .. }
                        | Command::StartProgram { .. }
                        | Command::ResumeProgram
                        | Command::Jog(_)
                        | Command::Home
                        | Command::SetSetting { .. }
                )),
                "{sent:?}"
            );
        }
    }

    #[test]
    fn a_pending_tool_length_write_blocks_a_second_one_until_it_resolves() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed write");
        assert!(model.tool_length_write_in_flight());
        let ctx = context();
        assert!(
            !model.select_card(ProbeCard::EdgeX, &ctx),
            "an in-flight TLO write cannot be discarded"
        );
        assert!(!model.close_card());

        assert!(
            model
                .request_tool_length_action(ToolLengthAction::Clear)
                .is_err()
        );
        assert!(
            model
                .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
                .is_err()
        );
        assert_eq!(sent.len(), 2, "no second line was sent");

        assert!(model.on_offsets_reported(Some(6.0)));
        assert!(model.close_card(), "a resolved write allows navigation");
        assert!(
            model
                .request_tool_length_action(ToolLengthAction::Clear)
                .is_err(),
            "leaving the card discards the run"
        );
    }

    #[test]
    fn tool_length_read_back_timeout_fails_and_keeps_the_old_value() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed write");

        assert!(!model.check_tool_length_timeout(Instant::now()));
        assert!(model.check_tool_length_timeout(Instant::now() + TOOL_LENGTH_READBACK_TIMEOUT));
        let write = model.tool_length_write().expect("write");
        assert!(
            matches!(
                &write.status,
                ToolLengthWriteStatus::Failed { message } if message.contains("timed out")
            ),
            "{:?}",
            write.status
        );
        assert_eq!(
            model
                .tool_length_preview()
                .expect("old preview retained")
                .current_tlo_mm,
            4.0
        );
    }

    #[test]
    fn clear_tool_length_sends_g49_and_verifies_zero() {
        let mut model = completed_tool_length(4.0, -10.0, -8.0);
        model
            .request_tool_length_action(ToolLengthAction::Clear)
            .expect("open clear confirmation");
        assert_eq!(
            model.tool_length_confirmation(),
            Some(ToolLengthAction::Clear)
        );
        let mut sent = Vec::new();
        let write = model
            .confirm_tool_length_action(ready_tlo_gate(), recorded_send(&mut sent))
            .expect("confirmed clear");
        assert_eq!(
            sent,
            vec![
                Command::Mdi("G49".to_string()),
                Command::System(SystemCommand::Offsets),
            ]
        );
        assert_eq!(write.target_tlo_mm, 0.0);
        assert!(model.on_offsets_reported(Some(0.0)));
        assert!(matches!(
            model.tool_length_write().map(|write| &write.status),
            Some(ToolLengthWriteStatus::Applied {
                reported_tlo_mm: 0.0
            })
        ));
    }

    #[test]
    fn existing_wcs_apply_still_works_for_an_edge_cycle() {
        let ctx = context();
        let mut model = model(ProbeCard::EdgeX);
        model
            .set_field_text(ProbeField::ExpectedX, "30.000")
            .expect("expected X");
        let setup = model.prepare(&ctx).expect("edge plan");
        let name = setup.plan.name.clone();
        model.begin(setup);
        assert!(model.on_job_loaded(&name));
        assert!(model.on_job_started());
        assert!(model.bind_result(ProbeResult::new(true, Position::xyz(27.0, 20.0, -1.0))));
        assert!(model.on_job_completed());
        let lines = model.apply_lines().expect("G10 apply");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("G10 L2 P1 X"), "{lines:?}");
        assert!(!lines[0].contains('$'));
        assert!(
            model.tool_length_preview().is_err(),
            "a WCS edge cycle is not a TLO cycle"
        );
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
            ProbeCard::CenterRectangle,
            ProbeCard::Rotation,
            ProbeCard::XyTouchPlate,
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

    // ------------------------------------------- per-profile probe defaults

    fn profile_defaults() -> ProbeDefaultsRecord {
        ProbeDefaultsRecord {
            safe_z_mm: 22.5,
            search_distance_mm: 6.5,
            feed_mm_min: 220.25,
            retract_mm: 1.5,
            probe_diameter_mm: 2.0,
            edge_offset_mm: 0.25,
            plate_thickness_mm: 9.5,
            ..ProbeDefaultsRecord::default()
        }
    }

    fn shared_text(model: &ProbeModel) -> Vec<String> {
        ProbeField::PROFILE_DEFAULTS
            .iter()
            .map(|field| model.field_text(*field).to_string())
            .collect()
    }

    #[test]
    fn apply_defaults_sets_every_shared_field_with_three_decimals() {
        let mut model = ProbeModel::default();
        // A stale inline error on a shared field is cleared by the load.
        assert!(model.set_field_text(ProbeField::Search, "bad").is_err());
        assert!(model.field_error(ProbeField::Search).is_some());

        assert!(model.apply_defaults(&profile_defaults()));
        assert_eq!(
            shared_text(&model),
            vec![
                "22.500".to_string(),
                "6.500".to_string(),
                "220.250".to_string(),
                "1.500".to_string(),
                "2.000".to_string(),
                "0.250".to_string(),
                "9.500".to_string(),
            ]
        );
        for field in ProbeField::PROFILE_DEFAULTS {
            assert_eq!(model.field_error(field), None, "{field:?}");
        }
    }

    #[test]
    fn apply_defaults_leaves_every_anchor_and_expected_value_alone() {
        let mut model = ProbeModel::default();
        for (field, text) in [
            (ProbeField::ExpectedZ, "-5.000"),
            (ProbeField::ProbingHeight, "1.000"),
            (ProbeField::ExpectedX, "20.000"),
            (ProbeField::ExpectedY, "21.000"),
            (ProbeField::CenterX, "2.000"),
            (ProbeField::CenterY, "3.000"),
            (ProbeField::Radius, "25.000"),
            (ProbeField::Touches, "6"),
            (ProbeField::Separation, "55.000"),
            (ProbeField::ReferenceZ, "-4.000"),
            (ProbeField::OriginX, "1.000"),
            (ProbeField::OriginY, "2.000"),
            (ProbeField::StepX, "11.000"),
            (ProbeField::StepY, "12.000"),
            (ProbeField::Columns, "5"),
            (ProbeField::Rows, "6"),
            (ProbeField::SurfaceZ, "4.000"),
            (ProbeField::ProbeOffsetX, "1.500"),
            (ProbeField::ProbeOffsetY, "-2.500"),
            (ProbeField::MapZOffset, "0.250"),
            (ProbeField::RectangleWidth, "65.000"),
            (ProbeField::RectangleHeight, "45.000"),
            (ProbeField::PlateX, "30.000"),
            (ProbeField::PlateY, "31.000"),
        ] {
            model
                .set_field_text(field, text)
                .unwrap_or_else(|error| panic!("{field:?}: {error}"));
        }
        let anchors: Vec<String> = ProbeField::ALL
            .iter()
            .filter(|field| !ProbeField::PROFILE_DEFAULTS.contains(field))
            .map(|field| model.field_text(*field).to_string())
            .collect();

        assert!(model.apply_defaults(&profile_defaults()));
        let after: Vec<String> = ProbeField::ALL
            .iter()
            .filter(|field| !ProbeField::PROFILE_DEFAULTS.contains(field))
            .map(|field| model.field_text(*field).to_string())
            .collect();
        assert_eq!(after, anchors);
    }

    #[test]
    fn apply_defaults_is_refused_while_a_probe_run_is_active() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("z plan");
        model.begin(setup);
        assert!(model.is_running());
        let before = shared_text(&model);

        assert!(!model.apply_defaults(&profile_defaults()));
        assert_eq!(shared_text(&model), before);
    }

    #[test]
    fn opening_a_card_after_a_profile_load_keeps_the_shared_defaults() {
        let ctx = context();
        let mut model = ProbeModel::default();
        assert!(model.apply_defaults(&profile_defaults()));
        assert!(model.select_card(ProbeCard::EdgeX, &ctx));
        assert_eq!(
            shared_text(&model),
            vec![
                "22.500".to_string(),
                "6.500".to_string(),
                "220.250".to_string(),
                "1.500".to_string(),
                "2.000".to_string(),
                "0.250".to_string(),
                "9.500".to_string(),
            ],
            "card selection reseeds anchors, not profile-owned shared values"
        );
    }

    #[test]
    fn defaults_record_parses_the_shared_fields_and_validates_the_record() {
        let mut model = ProbeModel::default();
        assert!(model.apply_defaults(&profile_defaults()));
        assert_eq!(model.defaults_record().expect("record"), profile_defaults());

        // The probe record requires positive search/feed/retract even though
        // the generic field editor allows zero as an intermediate value.
        model
            .set_field_text(ProbeField::Search, "0")
            .expect("zero stores");
        let error = model.defaults_record().expect_err("zero search refused");
        assert!(error.contains("SEARCH"), "{error}");
        assert_eq!(model.field_text(ProbeField::Search), "0");
    }

    #[test]
    fn defaults_record_names_a_non_finite_field_and_keeps_the_text() {
        let model = ProbeModel::default();
        let index = ProbeField::SafeZ.index();
        let mut broken = model.clone();
        broken.fields[index] = "NaN".to_string();
        let error = broken.defaults_record().expect_err("NaN refused");
        assert!(error.contains("SAFE Z"), "{error}");
        assert_eq!(broken.field_text(ProbeField::SafeZ), "NaN");

        let mut infinite = ProbeModel::default();
        infinite.fields[index] = "inf".to_string();
        let error = infinite.defaults_record().expect_err("inf refused");
        assert!(error.contains("SAFE Z"), "{error}");
        assert_eq!(infinite.field_text(ProbeField::SafeZ), "inf");
    }

    // ------------------------------------------------------ probe macros

    #[test]
    fn apply_defaults_loads_both_probe_macros() {
        let mut defaults = profile_defaults();
        defaults.pre_probe_macro = "(pre-probe)\nG4 P1".to_string();
        defaults.post_probe_macro = "; post-probe\nM5".to_string();

        let mut model = ProbeModel::default();
        assert!(model.apply_defaults(&defaults));
        assert_eq!(model.pre_macro(), "(pre-probe)\nG4 P1");
        assert_eq!(model.post_macro(), "; post-probe\nM5");

        // An invalid macro text refuses the whole record and never clobbers
        // the macros the model already holds.
        let mut invalid = defaults.clone();
        invalid.pre_probe_macro = "G0 X0".to_string();
        assert!(!model.apply_defaults(&invalid));
        assert_eq!(model.pre_macro(), "(pre-probe)\nG4 P1");
        assert_eq!(model.post_macro(), "; post-probe\nM5");
    }

    #[test]
    fn defaults_record_preserves_existing_probe_macros() {
        let mut model = ProbeModel::default();
        model
            .set_probe_macros("(pre)\nG4 P2", "; post\nM9")
            .expect("valid macros");
        let record = model.defaults_record().expect("record");
        assert_eq!(record.pre_probe_macro, "(pre)\nG4 P2");
        assert_eq!(record.post_probe_macro, "; post\nM9");
    }

    #[test]
    fn set_probe_macros_accepts_and_normalizes_multiline_text() {
        let mut model = ProbeModel::default();
        model
            .set_probe_macros("  (pre-probe)  \n\n\tM5\t\n", "; post\r\n\r\n  G4 P1.5  \n")
            .expect("multi-line macros");
        assert_eq!(model.pre_macro(), "(pre-probe)\nM5");
        assert_eq!(model.post_macro(), "; post\nG4 P1.5");
        assert_eq!(model.probe_macro_summary(), "PRE 2 / POST 2 LINES");
        assert_eq!(ProbeModel::default().probe_macro_summary(), "EMPTY");
    }

    #[test]
    fn forbidden_probe_macro_lines_are_rejected_with_their_number() {
        let mut model = ProbeModel::default();
        model
            .set_probe_macros("(pre)", "M9")
            .expect("initial macros");
        for forbidden in [
            "G0 X0",
            "G38.2 Z-1",
            "M3 S1000",
            "M6 T1",
            "G10 L2 P1 X0",
            "$H",
        ] {
            let error = model
                .set_probe_macros(&format!("(pre)\n{forbidden}"), "M9")
                .expect_err("forbidden line");
            assert!(error.contains("line 2"), "{forbidden}: {error}");
            assert!(error.contains(forbidden), "{forbidden}: {error}");
            assert_eq!(model.pre_macro(), "(pre)");
            assert_eq!(model.post_macro(), "M9");
        }
    }

    #[test]
    fn probe_macro_dwell_over_sixty_seconds_is_rejected() {
        let mut model = ProbeModel::default();
        let error = model
            .set_probe_macros("G4 P61", "")
            .expect_err("dwell bound");
        assert!(error.contains("line 1"), "{error}");
        assert!(error.contains("0..=60"), "{error}");
        assert_eq!(model.pre_macro(), "");
    }

    #[test]
    fn probe_macro_byte_and_line_bounds_are_rejected() {
        let mut model = ProbeModel::default();
        let too_long = format!("({})", "a".repeat(MAX_PROBE_MACRO_BYTES));
        let error = model
            .set_probe_macros(&too_long, "")
            .expect_err("byte bound");
        assert!(error.contains("limit"), "{error}");

        let too_many_lines: String = (0..=MAX_PROBE_MACRO_LINES)
            .map(|index| format!("(line {index})\n"))
            .collect();
        let error = model
            .set_probe_macros("", &too_many_lines)
            .expect_err("line bound");
        assert!(error.contains("lines"), "{error}");
        assert_eq!(model.pre_macro(), "");
        assert_eq!(model.post_macro(), "");
    }

    #[test]
    fn preflight_plan_places_pre_after_preamble_and_post_at_the_end() {
        let ctx = context();
        let mut macro_model = model(ProbeCard::ZTouchPlate);
        macro_model
            .set_probe_macros("(pre-probe)\nG4 P1", "(post-probe)\nM9")
            .expect("valid macros");
        let setup = macro_model.prepare(&ctx).expect("macro plan");

        let lines: Vec<&str> = setup.plan.program.lines().collect();
        assert_eq!(lines[0], "G21 G90 G17 G94", "{lines:?}");
        assert_eq!(lines[1], "G54", "{lines:?}");
        assert_eq!(lines[2], "M5", "{lines:?}");
        assert_eq!(lines[3], "M9", "{lines:?}");
        assert!(lines[4].starts_with("G0 Z"), "{lines:?}");
        assert_eq!(lines[5], "(pre-probe)", "{lines:?}");
        assert_eq!(lines[6], "G4 P1", "{lines:?}");
        assert_eq!(lines[lines.len() - 2], "(post-probe)", "{lines:?}");
        assert_eq!(lines[lines.len() - 1], "M9", "{lines:?}");

        // The macros are text only: the expected probe count and probe list
        // are exactly the plain plan's.
        let plain = model(ProbeCard::ZTouchPlate);
        let plain_setup = plain.prepare(&ctx).expect("plain plan");
        assert_eq!(setup.plan.expected_probes, plain_setup.plan.expected_probes);
        assert_eq!(setup.plan.probes, plain_setup.plan.probes);
    }

    #[test]
    fn empty_probe_macros_are_the_identity_path() {
        let ctx = context();
        let mut blank = model(ProbeCard::EdgeX);
        blank
            .set_probe_macros(" \n\t\n", "\n")
            .expect("blank macros");
        let blank_setup = blank.prepare(&ctx).expect("blank plan");
        let plain_setup = model(ProbeCard::EdgeX).prepare(&ctx).expect("plain plan");
        assert_eq!(blank_setup.plan.program, plain_setup.plan.program);
        assert_eq!(blank_setup.plan.probes, plain_setup.plan.probes);
        assert_eq!(blank.probe_macro_summary(), "EMPTY");
    }

    #[test]
    fn height_map_plan_includes_the_same_macros() {
        let ctx = context();
        let mut model = model(ProbeCard::HeightMap);
        set_height_grid(&mut model, 2, 2);
        model
            .set_probe_macros("(pre-map)", "(post-map)")
            .expect("valid macros");
        let setup = model.prepare(&ctx).expect("height-map plan");
        let lines: Vec<&str> = setup.plan.program.lines().collect();
        assert_eq!(lines[5], "(pre-map)", "{lines:?}");
        assert_eq!(lines[lines.len() - 1], "(post-map)", "{lines:?}");
    }

    #[test]
    fn macro_plan_rejection_keeps_the_previous_plan_and_run() {
        let ctx = context();
        let mut model = model(ProbeCard::ZTouchPlate);
        let setup = model.prepare(&ctx).expect("plain plan");
        model.begin(setup);
        let run_before = model
            .run()
            .map(|run| run.setup.plan.program.clone())
            .expect("run");

        // Bypass the editor guard to prove the plan boundary maps a
        // `ProbeMacroError` to the preflight string and mutates nothing.
        model.pre_macro = "G0 X0".to_string();
        let error = model.prepare(&ctx).expect_err("plan refused");
        assert!(error.contains("line 1"), "{error}");
        assert!(error.contains("G0 X0"), "{error}");
        assert_eq!(model.pre_macro(), "G0 X0");
        assert_eq!(
            model.run().map(|run| run.setup.plan.program.clone()),
            Some(run_before)
        );
        assert!(model.is_running());
    }
}
