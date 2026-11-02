//! Pure probing plans and result math for the milling workflow.
//!
//! This crate builds bounded G-code plans, validates their safety parameters,
//! and computes offsets/centers/height maps from probe results. It performs no
//! I/O and never writes to a controller; the engine's job runner streams the
//! plans and the app owns preflight confirmation.
//!
//! # Conventions
//!
//! - All lengths are millimetres. Cycle geometry and computed results are in
//!   **machine coordinates**, matching `PRB` and `G10` offsets.
//!   [`ProbeParams::wcs_offset_mm`] is the active work offset
//!   (`machine = work + offset`); the plan subtracts it so the emitted program
//!   runs in [`ProbeParams::wcs`]. Set it from
//!   [`ghs_core::CoordinateSystem::work_offset`] at preflight, not to zero by
//!   default, unless the WCS really has no offset.
//! - Plans force `G21 G90 G17 G94`, so the controller interprets every number
//!   as millimetres regardless of [`ProbeParams::units`]. The `units` field is
//!   a display preference only; convert operator inch input with
//!   [`ghs_core::Units::to_mm`] before building parameters.
//! - The XY plane is `G17`. Z is positive up. Every plan moves to
//!   [`ProbeParams::safe_z_mm`] (a machine coordinate) before and after a
//!   search.
//! - A search starts away from the expected contact and travels at most
//!   [`ProbeParams::search_distance_mm`]. The target of the `G38.2` move is the
//!   expected *probe-centre contact* (machine coordinates): a missing surface
//!   stops at that target and the controller raises a probe-failure alarm
//!   (grblHAL alarm 4 or 5), which stops the job. Set the expected value to the
//!   farthest plausible contact in the approach direction; closer contacts
//!   simply trigger early. For a downward Z probe that is the lowest plausible
//!   surface; for a +X edge it is the largest plausible X. Every start is one
//!   search distance before that target, so a missed probe never travels past
//!   the expected contact.
//! - Every plan uses `G38.2` (towards the surface, fail closed). `G38.3` and
//!   `G38.5` continue after a miss and `G38.4` probes away; none is emitted by
//!   a cycle. See [`ProbeMoveCode`] for the full grblHAL family.
//! - Plans contain `M5` and `M9` and never contain `M3`, `M4`, `M7` or `M8`.
//!   They never contain a cycle-start character; the app starts the program.
//! - Raw [`ghs_core::ProbeResult`] positions are machine coordinates of the
//!   **controlled point**. Result math keeps that frame and reports machine
//!   coordinates for the app to convert or to write with `G10 L2`/`G10 L20`
//!   after confirmation.
//! - A probe may be mounted away from the controlled point. The optional
//!   probe-tip offset is `offset = probe tip - controlled point`, in machine
//!   or work XY. Cycle geometry in [`ProbeParams`] and
//!   [`HeightMapGrid`] is **feature / probe-tip** coordinates; the plan emits
//!   `controlled = feature - offset` for every XY move. Result math adds
//!   `offset` back to raw `PRB` XY before computing edge/corner/circle/
//!   rectangle/rotation/XY-plate values. Raw samples stay raw for audit.
//!   [`ProbeCycle::plan_with_offset`],
//!   [`HeightMapGrid::plan_with_offset`], and
//!   [`ProbeOutcome::from_results_with_offset`] take the offset; the original
//!   methods use zero.
//!
//! # Height map
//!
//! [`HeightMapGrid`] builds one bounded `G38.2` program for a row-major grid.
//! Every point starts at [`ProbeParams::safe_z_mm`] (the rapid plane above the
//! whole grid) and probes down to the nominal lowest surface, so a locally
//! higher area is met by the probe feed instead of a rapid. The vertical travel
//! is `safe_z - surface_z` and must fit within
//! [`ProbeParams::search_distance_mm`]. Points retract to safe Z between
//! searches, and `G38.2` stops the job on a miss, so a failed point never runs
//! the rest of the grid. [`HeightMap`] stores one
//! `Option<f64>` per cell, interpolates bilinearly only when every required
//! sample is valid, and offers [`HeightMap::apply_to_z`] as the compensation
//! primitive. [`HeightMap::compensate_toolpath`] applies the map to a viewer
//! toolpath, subdividing long moves for sampling and reporting compensated
//! and uncompensated length. It takes a caller-supplied work offset because
//! lookups are machine XY; rotation and scaling are deliberately not modelled.
//! A map may also carry a constant Z datum shift set with
//! [`HeightMap::with_z_offset`] (or [`HeightMap::set_z_offset`]): raw
//! [`HeightMap::samples`] stay unchanged for persistence, while `z`,
//! `height_at`, `valid_points`, `z_range`, `summary`, `csv`, `apply_to_z`, and
//! `compensate_toolpath` report effective heights including the offset.
//! [`HeightMap::summary`] and [`HeightMap::csv`] are pure in-memory exports.
//! Writing a corrected `.nc` file is deferred; the app should stream
//! compensation or export in a later milestone.
//!
//! # Probe radius
//!
//! A touch is reported at the controlled point; after adding any probe-tip
//! offset it is the probe-tip centre. The contacted edge is on the leading
//! side of that centre:
//!
//! `edge = contact + approach_sign * probe_radius`
//!
//! A work zero placed `edge_offset` into the material (the approach direction
//! while probing a face) is:
//!
//! `zero = edge + approach_sign * edge_offset`
//!
//! # Example
//!
//! ```
//! use ghs_core::{Position, Wcs};
//! use ghs_probe::{
//!     EdgeMode, EdgeSpec, ProbeAxis, ProbeCycle, ProbeParams, Sign,
//! };
//!
//! let params = ProbeParams {
//!     safe_z_mm: 10.0,
//!     search_distance_mm: 5.0,
//!     feed_mm_min: 100.0,
//!     retract_mm: 2.0,
//!     probe_diameter_mm: 4.0,
//!     position: Some(Position::xyz(0.0, 0.0, -2.0)),
//!     wcs: Wcs::G54,
//!     ..ProbeParams::default()
//! };
//! let cycle = ProbeCycle::Edge(EdgeSpec {
//!     axis: ProbeAxis::X,
//!     mode: EdgeMode::Outside,
//!     approach: Sign::Positive,
//!     expected_edge_mm: 20.0,
//! });
//! let plan = cycle.plan(&params).unwrap();
//! assert!(plan.program.starts_with("G21 G90 G17 G94"));
//! assert!(plan.program.contains("G38.2 X18.000 F100.000"));
//! assert_eq!(plan.expected_probes, 1);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cycle;
mod error;
mod gcode;
mod heightmap;
mod math;
mod params;
mod result;

pub use cycle::{CornerProbe, EdgeSpec, ProbeCycle, RotationProbe};
pub use error::{ProbeError, ValidationError};
pub use gcode::{PlannedProbe, ProbeMoveCode, ProbePlan, ProbePreconditions, ProbeVector};
pub use heightmap::{
    CompensatedToolpath, HEIGHT_MAP_POINT_OVERHEAD_SECONDS, HeightMap, HeightMapEstimate,
    HeightMapGrid, HeightMapSummary, MAX_COMPENSATED_SEGMENTS, MAX_HEIGHT_MAP_AXIS_POINTS,
    MAX_HEIGHT_MAP_POINTS, MAX_HEIGHT_MAP_Z_OFFSET_MM,
};
pub use math::{
    CircleFit, RectangleFit, RotationAxis, RotationFit, circle_feature_radius, corner_work_zero,
    edge_from_contact, edge_work_zero, fit_circle, rectangle_center_from_edges,
    rectangle_center_from_touches, rotation_from_points, tool_length_offset, xy_plate_zero,
    z_zero_from_contact,
};
pub use params::{
    EdgeMode, MAX_CIRCLE_TOUCHES, MAX_COORDINATE_MM, MAX_EDGE_OFFSET_MM, MAX_PLATE_THICKNESS_MM,
    MAX_PROBE_DIAMETER_MM, MAX_PROBE_FEED_MM_MIN, MAX_PROBE_OFFSET_MM, MAX_RADIUS_MM,
    MAX_RECTANGLE_MM, MAX_RETRACT_MM, MAX_SEARCH_DISTANCE_MM, MAX_SEPARATION_MM,
    MIN_CIRCLE_TOUCHES, ProbeAxis, ProbeParams, Sign, XyPoint, validate_probe_offset,
};
pub use result::{CycleKind, ProbeOutcome, ProbeSample, ProbeValue};

/// Version of the probe crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
