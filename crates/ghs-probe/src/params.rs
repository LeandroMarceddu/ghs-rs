//! Shared probe parameters and small geometry types.

use ghs_core::{Axis, Position, Units, Wcs};

use crate::error::ValidationError;

/// Largest accepted search distance, in millimetres.
pub const MAX_SEARCH_DISTANCE_MM: f64 = 100.0;

/// Largest accepted probe feed, in millimetres per minute.
pub const MAX_PROBE_FEED_MM_MIN: f64 = 2_000.0;

/// Largest accepted retract distance, in millimetres.
pub const MAX_RETRACT_MM: f64 = 100.0;

/// Largest accepted probe diameter, in millimetres.
pub const MAX_PROBE_DIAMETER_MM: f64 = 50.0;

/// Largest accepted edge offset, in millimetres.
pub const MAX_EDGE_OFFSET_MM: f64 = 100.0;

/// Largest accepted absolute coordinate, in millimetres.
pub const MAX_COORDINATE_MM: f64 = 100_000.0;

/// Largest accepted feature radius, in millimetres.
pub const MAX_RADIUS_MM: f64 = 1_000.0;

/// Largest accepted touch-plate thickness, in millimetres.
pub const MAX_PLATE_THICKNESS_MM: f64 = 100.0;

/// Largest accepted rotation point separation, in millimetres.
pub const MAX_SEPARATION_MM: f64 = 1_000.0;

/// Largest accepted rectangle side, in millimetres.
pub const MAX_RECTANGLE_MM: f64 = 10_000.0;

/// Smallest number of circle touches.
pub const MIN_CIRCLE_TOUCHES: u8 = 3;

/// Largest number of circle touches.
pub const MAX_CIRCLE_TOUCHES: u8 = 8;

/// A point in the XY plane, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct XyPoint {
    /// X coordinate in millimetres.
    pub x_mm: f64,
    /// Y coordinate in millimetres.
    pub y_mm: f64,
}

impl XyPoint {
    /// Build a point from millimetre coordinates.
    #[must_use]
    pub const fn new(x_mm: f64, y_mm: f64) -> Self {
        XyPoint { x_mm, y_mm }
    }

    /// Project a 3D position onto XY.
    #[must_use]
    pub fn from_position(position: Position) -> Self {
        XyPoint::new(position.get(Axis::X), position.get(Axis::Y))
    }

    /// Returns `true` when both coordinates are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x_mm.is_finite() && self.y_mm.is_finite()
    }

    /// Euclidean distance to `other`.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        let dx = self.x_mm - other.x_mm;
        let dy = self.y_mm - other.y_mm;
        dx.hypot(dy)
    }

    /// Midpoint between two points.
    #[must_use]
    pub fn midpoint(self, other: Self) -> Self {
        XyPoint::new(
            self.x_mm * 0.5 + other.x_mm * 0.5,
            self.y_mm * 0.5 + other.y_mm * 0.5,
        )
    }

    /// Convert to the viewer's `f32` point for an overlay.
    #[must_use]
    pub fn to_viewer(self) -> ghs_viewer::Point2 {
        ghs_viewer::Point2::new(self.x_mm as f32, self.y_mm as f32)
    }
}

/// Direction sign along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sign {
    /// Increasing coordinate.
    Positive,
    /// Decreasing coordinate.
    Negative,
}

impl Sign {
    /// Numeric sign, `1.0` or `-1.0`.
    #[must_use]
    pub const fn as_f64(self) -> f64 {
        match self {
            Sign::Positive => 1.0,
            Sign::Negative => -1.0,
        }
    }

    /// Short display form, `"+"` or `"-"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Sign::Positive => "+",
            Sign::Negative => "-",
        }
    }

    /// The opposite direction.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Sign::Positive => Sign::Negative,
            Sign::Negative => Sign::Positive,
        }
    }
}

/// A horizontal axis used by edge, corner, rectangle and rotation probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeAxis {
    /// X axis.
    X,
    /// Y axis.
    Y,
}

impl ProbeAxis {
    /// Upper-case axis letter.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProbeAxis::X => "X",
            ProbeAxis::Y => "Y",
        }
    }

    /// Domain axis.
    #[must_use]
    pub const fn axis(self) -> Axis {
        match self {
            ProbeAxis::X => Axis::X,
            ProbeAxis::Y => Axis::Y,
        }
    }

    /// The perpendicular horizontal axis.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            ProbeAxis::X => ProbeAxis::Y,
            ProbeAxis::Y => ProbeAxis::X,
        }
    }

    /// Read this axis from a position.
    #[must_use]
    pub fn component(self, position: Position) -> f64 {
        position.get(self.axis())
    }

    /// Copy a position with this axis replaced.
    #[must_use]
    pub fn replace(self, position: Position, value: f64) -> Position {
        let mut result = position;
        result.set(self.axis(), value);
        result
    }
}

/// Whether the probe is outside a part or inside a pocket/bore.
///
/// The mode describes the physical side the cycle starts on and is reported for
/// the preflight sheet. The radius correction itself is derived from the
/// approach direction, because the touched surface is always on the leading
/// side of the probe centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeMode {
    /// The probe starts outside the material and moves towards it.
    Outside,
    /// The probe starts inside a pocket or bore and moves towards its wall.
    Inside,
}

impl EdgeMode {
    /// Lower-case name for logs and plan names.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            EdgeMode::Outside => "outside",
            EdgeMode::Inside => "inside",
        }
    }
}

/// Shared probing parameters.
///
/// Every length is in millimetres. Geometry fields (`position`, `center`,
/// `wcs_offset_mm`, `safe_z_mm`) are machine coordinates; the generated
/// program subtracts [`ProbeParams::wcs_offset_mm`] so it runs in
/// [`ProbeParams::wcs`]. The app must convert inch input before constructing
/// this value. [`ProbeParams::units`] only tells the app which display unit the
/// operator requested.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeParams {
    /// Safe Z plane, in machine millimetres. The plan moves here before and
    /// after each search and never starts a vertical search above it.
    pub safe_z_mm: f64,
    /// Maximum search travel, in millimetres. Must be positive and at most
    /// [`MAX_SEARCH_DISTANCE_MM`].
    pub search_distance_mm: f64,
    /// Probe feed, in millimetres per minute. Must be positive and at most
    /// [`MAX_PROBE_FEED_MM_MIN`].
    pub feed_mm_min: f64,
    /// Relative Z lift after a touch, in millimetres, before the move to safe Z.
    pub retract_mm: f64,
    /// Probe tip diameter, in millimetres. Zero is allowed for a zero-radius
    /// stylus; the radius is `diameter / 2`.
    pub probe_diameter_mm: f64,
    /// Distance from the measured edge to the new work zero, measured into the
    /// material along the approach direction.
    pub edge_offset_mm: f64,
    /// Cycle anchor, in machine coordinates. For Z touch-plate and tool-length
    /// cycles this is the XY position above the plate and is optional (the
    /// current XY is used). For edge, corner, rotation and XY-plate cycles it
    /// is required; its Z is the probing height.
    pub position: Option<Position>,
    /// Approximate centre for circle and rectangle cycles, in machine
    /// coordinates. Its Z is the probing height.
    pub center: Option<Position>,
    /// Expected feature radius for a circle cycle, in millimetres.
    pub radius_mm: Option<f64>,
    /// Active work offset, `machine = work + offset`, in millimetres.
    ///
    /// Cycle geometry and results are machine coordinates (matching `PRB` and
    /// `G10` offsets). The plan subtracts this offset to express every move in
    /// [`ProbeParams::wcs`]. Set it from
    /// [`ghs_core::CoordinateSystem::work_offset`] for the selected WCS
    /// (including `G92`); when the plan selects another WCS, use that WCS
    /// offset plus `G92`. If a `G43`/`G43.1` tool-length offset is active and is
    /// not cancelled before the run, add its Z component here for Z moves; the
    /// plan never emits `G49`, so the app must decide and show the state. Zero
    /// is only correct for a WCS with no offset and no active TLO.
    pub wcs_offset_mm: Position,
    /// Work coordinate system selected by the generated program.
    pub wcs: Wcs,
    /// Display units requested by the operator. Values are still millimetres;
    /// the program always selects `G21`.
    pub units: Units,
}

impl Default for ProbeParams {
    fn default() -> Self {
        ProbeParams {
            safe_z_mm: 10.0,
            search_distance_mm: 5.0,
            feed_mm_min: 100.0,
            retract_mm: 2.0,
            probe_diameter_mm: 0.0,
            edge_offset_mm: 0.0,
            position: None,
            center: None,
            radius_mm: None,
            wcs_offset_mm: Position::zero(),
            wcs: Wcs::G54,
            units: Units::Mm,
        }
    }
}

impl ProbeParams {
    /// Probe tip radius, in millimetres.
    #[must_use]
    pub fn probe_radius_mm(&self) -> f64 {
        self.probe_diameter_mm * 0.5
    }

    /// Convert a machine position into the plan's work frame.
    ///
    /// Only axes present in `machine` are converted, so a Z-only probe record
    /// stays Z-only.
    #[must_use]
    pub fn to_work(&self, machine: Position) -> Position {
        let mut work = machine;
        for axis in machine.axes().iter() {
            work.set(axis, machine.get(axis) - self.wcs_offset_mm.get(axis));
        }
        work
    }

    /// Convert a work position into machine coordinates.
    #[must_use]
    pub fn to_machine(&self, work: Position) -> Position {
        let mut machine = work;
        for axis in work.axes().iter() {
            machine.set(axis, work.get(axis) + self.wcs_offset_mm.get(axis));
        }
        machine
    }

    /// Validate every shared parameter.
    ///
    /// Optional geometry is validated when present, even for a cycle that does
    /// not use it, so a stale field cannot hide bad input from the preflight.
    pub fn validate(&self) -> Result<(), ValidationError> {
        check_coordinate("safe_z_mm", self.safe_z_mm)?;
        check_range(
            "search_distance_mm",
            self.search_distance_mm,
            0.0,
            MAX_SEARCH_DISTANCE_MM,
        )?;
        if self.search_distance_mm <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "search_distance_mm",
                value: self.search_distance_mm,
            });
        }
        check_range("feed_mm_min", self.feed_mm_min, 0.0, MAX_PROBE_FEED_MM_MIN)?;
        if self.feed_mm_min <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "feed_mm_min",
                value: self.feed_mm_min,
            });
        }
        check_range("retract_mm", self.retract_mm, 0.0, MAX_RETRACT_MM)?;
        if self.retract_mm <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "retract_mm",
                value: self.retract_mm,
            });
        }
        check_range(
            "probe_diameter_mm",
            self.probe_diameter_mm,
            0.0,
            MAX_PROBE_DIAMETER_MM,
        )?;
        check_range(
            "edge_offset_mm",
            self.edge_offset_mm,
            0.0,
            MAX_EDGE_OFFSET_MM,
        )?;
        if let Some(position) = self.position {
            check_position("position", position)?;
        }
        if let Some(center) = self.center {
            check_position("center", center)?;
        }
        if let Some(radius) = self.radius_mm {
            check_range("radius_mm", radius, 0.0, MAX_RADIUS_MM)?;
            if radius <= 0.0 {
                return Err(ValidationError::NotPositive {
                    field: "radius_mm",
                    value: radius,
                });
            }
        }
        check_position("wcs_offset_mm", self.wcs_offset_mm)?;
        Ok(())
    }
}

/// Validate that a value is finite.
pub(crate) fn check_finite(field: &'static str, value: f64) -> Result<(), ValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ValidationError::NonFinite { field, value })
    }
}

/// Validate that a value lies in an inclusive range.
pub(crate) fn check_range(
    field: &'static str,
    value: f64,
    min: f64,
    max: f64,
) -> Result<(), ValidationError> {
    check_finite(field, value)?;
    if value < min || value > max {
        Err(ValidationError::OutOfRange {
            field,
            value,
            min,
            max,
        })
    } else {
        Ok(())
    }
}

/// Validate that a coordinate is finite and within the machine bound.
pub(crate) fn check_coordinate(field: &'static str, value: f64) -> Result<(), ValidationError> {
    check_range(field, value, -MAX_COORDINATE_MM, MAX_COORDINATE_MM)
}

/// Validate every present coordinate in a position.
pub(crate) fn check_position(
    field: &'static str,
    position: Position,
) -> Result<(), ValidationError> {
    for (_, value) in position.iter() {
        check_coordinate(field, value)?;
    }
    Ok(())
}
