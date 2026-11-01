//! Probe cycles and their bounded G-code plans.

use std::f64::consts::TAU;

use ghs_core::{Axes, Axis, Position};

use crate::error::{ProbeError, ValidationError};
use crate::gcode::GcodeBuilder;
use crate::params::{
    EdgeMode, MAX_CIRCLE_TOUCHES, MAX_PLATE_THICKNESS_MM, MAX_RADIUS_MM, MAX_RECTANGLE_MM,
    MAX_SEPARATION_MM, MIN_CIRCLE_TOUCHES, ProbeAxis, ProbeParams, Sign, check_coordinate,
    check_finite, check_range,
};
use crate::result::CycleKind;

/// One edge measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSpec {
    /// Axis being probed.
    pub axis: ProbeAxis,
    /// Whether the probe starts outside the part or inside a pocket/bore.
    pub mode: EdgeMode,
    /// Search direction along `axis`.
    pub approach: Sign,
    /// Farthest expected edge coordinate along `axis`, in machine
    /// millimetres.
    ///
    /// The search target is derived from this value minus the probe radius, so
    /// a missed probe stops one radius before this edge. Set this to the
    /// largest plausible coordinate in the approach direction; closer edges
    /// trigger earlier.
    pub expected_edge_mm: f64,
}

impl EdgeSpec {
    /// Build an edge specification.
    #[must_use]
    pub const fn new(
        axis: ProbeAxis,
        mode: EdgeMode,
        approach: Sign,
        expected_edge_mm: f64,
    ) -> Self {
        EdgeSpec {
            axis,
            mode,
            approach,
            expected_edge_mm,
        }
    }

    /// Validate the edge coordinate against the shared search geometry.
    pub fn validate(&self, params: &ProbeParams) -> Result<(), ValidationError> {
        check_coordinate("expected_edge_mm", self.expected_edge_mm)?;
        let hint = self.expected_edge_mm - self.approach.as_f64() * params.probe_radius_mm();
        check_coordinate("contact hint", hint)?;
        let start = hint - self.approach.as_f64() * params.search_distance_mm;
        check_coordinate("search start", start)?;
        Ok(())
    }

    /// Expected probe-centre contact along the edge axis.
    pub fn contact_hint_mm(&self, params: &ProbeParams) -> Result<f64, ProbeError> {
        let value = self.expected_edge_mm - self.approach.as_f64() * params.probe_radius_mm();
        check_finite("contact hint", value)?;
        Ok(value)
    }

    /// Search start along the edge axis.
    pub fn start_mm(&self, params: &ProbeParams) -> Result<f64, ProbeError> {
        let value =
            self.contact_hint_mm(params)? - self.approach.as_f64() * params.search_distance_mm;
        check_finite("search start", value)?;
        Ok(value)
    }

    /// Correct a contact coordinate to the measured edge.
    pub fn edge_mm(&self, params: &ProbeParams, contact_mm: f64) -> Result<f64, ProbeError> {
        crate::math::edge_from_contact(contact_mm, params.probe_radius_mm(), self.approach)
    }

    /// Work zero derived from a contact coordinate.
    pub fn zero_mm(&self, params: &ProbeParams, contact_mm: f64) -> Result<f64, ProbeError> {
        let edge = self.edge_mm(params, contact_mm)?;
        crate::math::edge_work_zero(edge, self.approach, params.edge_offset_mm)
    }
}

/// Two perpendicular edges for a corner or an XY touch plate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerProbe {
    /// X edge.
    pub x: EdgeSpec,
    /// Y edge.
    pub y: EdgeSpec,
}

impl CornerProbe {
    /// Build a corner from an X and a Y edge.
    #[must_use]
    pub const fn new(x: EdgeSpec, y: EdgeSpec) -> Self {
        CornerProbe { x, y }
    }

    /// Validate the axes and edge coordinates.
    pub fn validate(&self, params: &ProbeParams) -> Result<(), ValidationError> {
        if self.x.axis != ProbeAxis::X {
            return Err(ValidationError::WrongAxis {
                field: "corner X probe",
                expected: "X",
                actual: self.x.axis.as_str(),
            });
        }
        if self.y.axis != ProbeAxis::Y {
            return Err(ValidationError::WrongAxis {
                field: "corner Y probe",
                expected: "Y",
                actual: self.y.axis.as_str(),
            });
        }
        self.x.validate(params)?;
        self.y.validate(params)?;
        Ok(())
    }
}

/// Two-point rotation measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationProbe {
    /// Separation between the two points, along the nominal edge, in
    /// millimetres.
    pub separation_mm: f64,
    /// Nominal edge direction.
    pub edge_axis: ProbeAxis,
    /// Probing direction along the normal axis.
    pub approach: Sign,
    /// Farthest expected edge coordinate along the normal axis, in machine
    /// millimetres.
    ///
    /// Both points use this value as the search target. For a nominally
    /// straight edge set it to the farthest plausible point in the approach
    /// direction so a small tilt still triggers both probes.
    pub expected_edge_mm: f64,
}

impl RotationProbe {
    /// Validate the separation and expected edge.
    pub fn validate(&self, params: &ProbeParams) -> Result<(), ValidationError> {
        check_range("separation_mm", self.separation_mm, 0.0, MAX_SEPARATION_MM)?;
        if self.separation_mm <= 0.0 {
            return Err(ValidationError::NotPositive {
                field: "separation_mm",
                value: self.separation_mm,
            });
        }
        check_coordinate("expected_edge_mm", self.expected_edge_mm)?;
        let hint = self.contact_hint_mm(params);
        check_coordinate("contact hint", hint)?;
        let start = self.start_mm(params);
        check_coordinate("search start", start)?;
        Ok(())
    }

    fn normal_axis(&self) -> ProbeAxis {
        self.edge_axis.other()
    }

    fn contact_hint_mm(&self, params: &ProbeParams) -> f64 {
        self.expected_edge_mm - self.approach.as_f64() * params.probe_radius_mm()
    }

    fn start_mm(&self, params: &ProbeParams) -> f64 {
        self.contact_hint_mm(params) - self.approach.as_f64() * params.search_distance_mm
    }
}

/// A complete probing cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProbeCycle {
    /// Probe down onto a touch plate.
    ZTouchPlate {
        /// Expected Z of the plate top, in machine millimetres. The search
        /// target; set it to the lowest plausible surface.
        expected_plate_z_mm: f64,
        /// Plate thickness to subtract from the contact, in millimetres.
        plate_thickness_mm: f64,
    },
    /// Probe down onto a reference surface for a tool-length offset.
    ToolLength {
        /// Reference surface Z (the stored reference contact), in machine
        /// millimetres.
        reference_z_mm: f64,
    },
    /// Probe one edge.
    Edge(EdgeSpec),
    /// Probe two edges for a corner.
    Corner(CornerProbe),
    /// Probe a circle at three or more points.
    ///
    /// Touches are evenly spaced starting at 0 degrees; the approximate centre
    /// comes from [`ProbeParams::center`] and the expected radius from
    /// [`ProbeParams::radius_mm`].
    CenterCircle {
        /// Number of touches, `3..=8`.
        touch_count: u8,
        /// Whether the feature is a bore or a boss.
        mode: EdgeMode,
    },
    /// Probe four rectangle sides for the centre.
    ///
    /// The plan touches the midpoint of each side once (four edge touches).
    /// [`crate::rectangle_center_from_touches`] can derive the centre from four
    /// symmetric corner or side touch results; a corner-touch plan is a
    /// follow-up.
    CenterRectangle {
        /// Rectangle width, in millimetres.
        width_mm: f64,
        /// Rectangle height, in millimetres.
        height_mm: f64,
        /// Whether the rectangle is a pocket or a boss.
        mode: EdgeMode,
    },
    /// Probe two points on an edge for work rotation.
    Rotation(RotationProbe),
    /// Probe an X/Y touch plate.
    XyTouchPlate(CornerProbe),
}

impl ProbeCycle {
    /// Cycle kind, for result dispatch and UI cards.
    #[must_use]
    pub const fn kind(&self) -> CycleKind {
        match self {
            ProbeCycle::ZTouchPlate { .. } => CycleKind::ZTouchPlate,
            ProbeCycle::ToolLength { .. } => CycleKind::ToolLength,
            ProbeCycle::Edge(_) => CycleKind::Edge,
            ProbeCycle::Corner(_) => CycleKind::Corner,
            ProbeCycle::CenterCircle { .. } => CycleKind::CenterCircle,
            ProbeCycle::CenterRectangle { .. } => CycleKind::CenterRectangle,
            ProbeCycle::Rotation(_) => CycleKind::Rotation,
            ProbeCycle::XyTouchPlate(_) => CycleKind::XyTouchPlate,
        }
    }

    /// Number of probe results this cycle expects.
    #[must_use]
    pub const fn expected_probes(&self) -> usize {
        match self {
            ProbeCycle::ZTouchPlate { .. } | ProbeCycle::ToolLength { .. } => 1,
            ProbeCycle::Edge(_) => 1,
            ProbeCycle::Corner(_) | ProbeCycle::XyTouchPlate(_) => 2,
            ProbeCycle::CenterCircle { touch_count, .. } => *touch_count as usize,
            ProbeCycle::CenterRectangle { .. } => 4,
            ProbeCycle::Rotation(_) => 2,
        }
    }

    /// Stable labels for each probe, in execution order.
    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        match self {
            ProbeCycle::ZTouchPlate { .. } | ProbeCycle::ToolLength { .. } => {
                vec!["touch".to_string()]
            }
            ProbeCycle::Edge(_) => vec!["edge".to_string()],
            ProbeCycle::Corner(_) | ProbeCycle::XyTouchPlate(_) => {
                vec!["x-edge".to_string(), "y-edge".to_string()]
            }
            ProbeCycle::CenterCircle { touch_count, .. } => (0..*touch_count)
                .map(|index| format!("touch-{index}"))
                .collect(),
            ProbeCycle::CenterRectangle { .. } => vec![
                "x-plus".to_string(),
                "x-minus".to_string(),
                "y-plus".to_string(),
                "y-minus".to_string(),
            ],
            ProbeCycle::Rotation(_) => {
                vec!["point-1".to_string(), "point-2".to_string()]
            }
        }
    }

    /// Stable plan name.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            ProbeCycle::ZTouchPlate { .. } => "z-touch-plate".to_string(),
            ProbeCycle::ToolLength { .. } => "tool-length".to_string(),
            ProbeCycle::Edge(spec) => format!(
                "edge-{}-{}-{}",
                spec.axis.as_str().to_ascii_lowercase(),
                spec.mode.as_str(),
                sign_word(spec.approach)
            ),
            ProbeCycle::Corner(_) => "corner".to_string(),
            ProbeCycle::CenterCircle { mode, .. } => {
                format!("center-circle-{}", mode.as_str())
            }
            ProbeCycle::CenterRectangle { mode, .. } => {
                format!("center-rectangle-{}", mode.as_str())
            }
            ProbeCycle::Rotation(_) => "rotation".to_string(),
            ProbeCycle::XyTouchPlate(_) => "xy-touch-plate".to_string(),
        }
    }

    /// Validate the cycle against shared parameters.
    pub fn validate(&self, params: &ProbeParams) -> Result<(), ProbeError> {
        params.validate()?;
        match self {
            ProbeCycle::ZTouchPlate {
                expected_plate_z_mm,
                plate_thickness_mm,
            } => {
                check_coordinate("expected_plate_z_mm", *expected_plate_z_mm)?;
                check_range(
                    "plate_thickness_mm",
                    *plate_thickness_mm,
                    0.0,
                    MAX_PLATE_THICKNESS_MM,
                )?;
                validate_vertical_search(params, *expected_plate_z_mm)?;
            }
            ProbeCycle::ToolLength { reference_z_mm } => {
                check_coordinate("reference_z_mm", *reference_z_mm)?;
                validate_vertical_search(params, *reference_z_mm)?;
            }
            ProbeCycle::Edge(spec) => {
                let position = require_position(params)?;
                require_z_axis(position, "position Z")?;
                spec.validate(params)?;
                validate_probing_height(params, position.get(Axis::Z))?;
            }
            ProbeCycle::Corner(corner) | ProbeCycle::XyTouchPlate(corner) => {
                let position = require_position(params)?;
                require_z_axis(position, "position Z")?;
                corner.validate(params)?;
                validate_probing_height(params, position.get(Axis::Z))?;
            }
            ProbeCycle::CenterCircle { touch_count, mode } => {
                let center = require_center(params)?;
                require_z_axis(center, "center Z")?;
                let radius =
                    params
                        .radius_mm
                        .ok_or(ProbeError::Validation(ValidationError::Missing(
                            "radius_mm",
                        )))?;
                check_range("radius_mm", radius, 0.0, MAX_RADIUS_MM)?;
                if radius <= 0.0 {
                    return Err(ValidationError::NotPositive {
                        field: "radius_mm",
                        value: radius,
                    }
                    .into());
                }
                if !(MIN_CIRCLE_TOUCHES..=MAX_CIRCLE_TOUCHES).contains(touch_count) {
                    return Err(ValidationError::OutOfRange {
                        field: "touch_count",
                        value: f64::from(*touch_count),
                        min: f64::from(MIN_CIRCLE_TOUCHES),
                        max: f64::from(MAX_CIRCLE_TOUCHES),
                    }
                    .into());
                }
                if radius <= params.probe_radius_mm() {
                    return Err(ValidationError::ProbeTooLarge {
                        diameter_mm: params.probe_diameter_mm,
                        feature: "circle radius",
                    }
                    .into());
                }
                let contact_radius = match mode {
                    EdgeMode::Inside => radius - params.probe_radius_mm(),
                    EdgeMode::Outside => radius + params.probe_radius_mm(),
                };
                for axis in [Axis::X, Axis::Y] {
                    check_coordinate("circle contact", center.get(axis) + contact_radius)?;
                    check_coordinate("circle contact", center.get(axis) - contact_radius)?;
                }
                validate_probing_height(params, center.get(Axis::Z))?;
            }
            ProbeCycle::CenterRectangle {
                width_mm,
                height_mm,
                mode: _,
            } => {
                let center = require_center(params)?;
                require_z_axis(center, "center Z")?;
                check_range("width_mm", *width_mm, 0.0, MAX_RECTANGLE_MM)?;
                check_range("height_mm", *height_mm, 0.0, MAX_RECTANGLE_MM)?;
                if *width_mm <= 0.0 {
                    return Err(ValidationError::NotPositive {
                        field: "width_mm",
                        value: *width_mm,
                    }
                    .into());
                }
                if *height_mm <= 0.0 {
                    return Err(ValidationError::NotPositive {
                        field: "height_mm",
                        value: *height_mm,
                    }
                    .into());
                }
                if (*width_mm * 0.5) <= params.probe_radius_mm() {
                    return Err(ValidationError::ProbeTooLarge {
                        diameter_mm: params.probe_diameter_mm,
                        feature: "rectangle width",
                    }
                    .into());
                }
                if (*height_mm * 0.5) <= params.probe_radius_mm() {
                    return Err(ValidationError::ProbeTooLarge {
                        diameter_mm: params.probe_diameter_mm,
                        feature: "rectangle height",
                    }
                    .into());
                }
                validate_probing_height(params, center.get(Axis::Z))?;
            }
            ProbeCycle::Rotation(rotation) => {
                let position = require_position(params)?;
                require_z_axis(position, "position Z")?;
                rotation.validate(params)?;
                validate_probing_height(params, position.get(Axis::Z))?;
            }
        }
        Ok(())
    }

    /// Build the complete G-code plan.
    pub fn plan(&self, params: &ProbeParams) -> Result<crate::ProbePlan, ProbeError> {
        self.validate(params)?;
        match self {
            ProbeCycle::ZTouchPlate {
                expected_plate_z_mm,
                ..
            } => build_vertical(
                params,
                *expected_plate_z_mm,
                CycleKind::ZTouchPlate,
                "z-touch-plate",
            ),
            ProbeCycle::ToolLength { reference_z_mm } => build_vertical(
                params,
                *reference_z_mm,
                CycleKind::ToolLength,
                "tool-length",
            ),
            ProbeCycle::Edge(spec) => build_edge(params, spec),
            ProbeCycle::Corner(corner) => {
                build_two_edges(params, corner, CycleKind::Corner, "corner")
            }
            ProbeCycle::CenterCircle { touch_count, mode } => {
                build_circle(params, *touch_count, *mode)
            }
            ProbeCycle::CenterRectangle {
                width_mm,
                height_mm,
                mode,
            } => build_rectangle(params, *width_mm, *height_mm, *mode),
            ProbeCycle::Rotation(rotation) => build_rotation(params, rotation),
            ProbeCycle::XyTouchPlate(corner) => {
                build_two_edges(params, corner, CycleKind::XyTouchPlate, "xy-touch-plate")
            }
        }
    }
}

fn require_position(params: &ProbeParams) -> Result<Position, ProbeError> {
    params
        .position
        .ok_or(ProbeError::Validation(ValidationError::Missing("position")))
}

fn require_center(params: &ProbeParams) -> Result<Position, ProbeError> {
    params
        .center
        .ok_or(ProbeError::Validation(ValidationError::Missing("center")))
}

fn require_z_axis(position: Position, field: &'static str) -> Result<(), ValidationError> {
    if position.axes().contains(Axis::Z) {
        Ok(())
    } else {
        Err(ValidationError::Missing(field))
    }
}

fn validate_vertical_search(params: &ProbeParams, expected_z_mm: f64) -> Result<(), ProbeError> {
    let start_z_mm = expected_z_mm + params.search_distance_mm;
    check_coordinate("search start Z", start_z_mm)?;
    if params.safe_z_mm < start_z_mm {
        return Err(ValidationError::SafeZTooLow {
            safe_z_mm: params.safe_z_mm,
            start_z_mm,
        }
        .into());
    }
    Ok(())
}

fn validate_probing_height(params: &ProbeParams, height_mm: f64) -> Result<(), ProbeError> {
    if height_mm > params.safe_z_mm {
        return Err(ValidationError::ProbingHeightAboveSafeZ {
            height_mm,
            safe_z_mm: params.safe_z_mm,
        }
        .into());
    }
    Ok(())
}

fn sign_word(sign: Sign) -> &'static str {
    match sign {
        Sign::Positive => "positive",
        Sign::Negative => "negative",
    }
}

fn z_only(z_mm: f64) -> Position {
    let mut position = Position::new(Axes::from(Axis::Z));
    position.set(Axis::Z, z_mm);
    position
}

fn build_vertical(
    params: &ProbeParams,
    expected_z_mm: f64,
    kind: CycleKind,
    name: &str,
) -> Result<crate::ProbePlan, ProbeError> {
    let start_z_mm = expected_z_mm + params.search_distance_mm;
    let mut builder = GcodeBuilder::new(params);
    let (from, to) = match params.position {
        Some(position) => {
            let x = position.get(Axis::X);
            let y = position.get(Axis::Y);
            builder.rapid_xy(x, y)?;
            builder.rapid_z(start_z_mm)?;
            (
                Position::xyz(x, y, start_z_mm),
                Position::xyz(x, y, expected_z_mm),
            )
        }
        None => {
            builder.rapid_z(start_z_mm)?;
            (z_only(start_z_mm), z_only(expected_z_mm))
        }
    };
    builder.probe("touch", from, to)?;
    builder.retract()?;
    builder.finish(kind, name)
}

fn build_edge(params: &ProbeParams, spec: &EdgeSpec) -> Result<crate::ProbePlan, ProbeError> {
    let position = require_position(params)?;
    let hint = spec.contact_hint_mm(params)?;
    let start = spec.start_mm(params)?;
    let from = spec.axis.replace(position, start);
    let to = spec.axis.replace(position, hint);
    let mut builder = GcodeBuilder::new(params);
    builder.rapid_xy(from.get(Axis::X), from.get(Axis::Y))?;
    builder.rapid_z(from.get(Axis::Z))?;
    builder.probe("edge", from, to)?;
    builder.retract()?;
    builder.finish(CycleKind::Edge, ProbeCycle::Edge(*spec).name())
}

fn build_two_edges(
    params: &ProbeParams,
    corner: &CornerProbe,
    kind: CycleKind,
    name: &str,
) -> Result<crate::ProbePlan, ProbeError> {
    let base = require_position(params)?;
    let x_hint = corner.x.contact_hint_mm(params)?;
    let x_start = corner.x.start_mm(params)?;
    let y_hint = corner.y.contact_hint_mm(params)?;
    let y_start = corner.y.start_mm(params)?;
    let z = base.get(Axis::Z);
    let x_from = Position::xyz(x_start, base.get(Axis::Y), z);
    let x_to = Position::xyz(x_hint, base.get(Axis::Y), z);
    let y_from = Position::xyz(base.get(Axis::X), y_start, z);
    let y_to = Position::xyz(base.get(Axis::X), y_hint, z);

    let mut builder = GcodeBuilder::new(params);
    builder.rapid_xy(x_from.get(Axis::X), x_from.get(Axis::Y))?;
    builder.rapid_z(z)?;
    builder.probe("x-edge", x_from, x_to)?;
    builder.retract()?;
    builder.rapid_xy(y_from.get(Axis::X), y_from.get(Axis::Y))?;
    builder.rapid_z(z)?;
    builder.probe("y-edge", y_from, y_to)?;
    builder.retract()?;
    builder.finish(kind, name)
}

fn build_circle(
    params: &ProbeParams,
    touch_count: u8,
    mode: EdgeMode,
) -> Result<crate::ProbePlan, ProbeError> {
    let center = require_center(params)?;
    let radius = params
        .radius_mm
        .ok_or(ProbeError::Validation(ValidationError::Missing(
            "radius_mm",
        )))?;
    let probe_radius = params.probe_radius_mm();
    let mut builder = GcodeBuilder::new(params);
    let count = usize::from(touch_count);
    let cx = center.get(Axis::X);
    let cy = center.get(Axis::Y);
    let cz = center.get(Axis::Z);
    let contact_radius = match mode {
        EdgeMode::Inside => radius - probe_radius,
        EdgeMode::Outside => radius + probe_radius,
    };
    for index in 0..count {
        let angle = TAU * (index as f64) / (count as f64);
        let unit_x = angle.cos();
        let unit_y = angle.sin();
        let start_radius = match mode {
            EdgeMode::Inside => (contact_radius - params.search_distance_mm).max(0.0),
            EdgeMode::Outside => contact_radius + params.search_distance_mm,
        };
        let start = Position::xyz(cx + unit_x * start_radius, cy + unit_y * start_radius, cz);
        let target = Position::xyz(
            cx + unit_x * contact_radius,
            cy + unit_y * contact_radius,
            cz,
        );
        builder.rapid_xy(start.get(Axis::X), start.get(Axis::Y))?;
        builder.rapid_z(cz)?;
        builder.probe(format!("touch-{index}"), start, target)?;
        builder.retract()?;
    }
    let name = format!("center-circle-{}", mode.as_str());
    builder.finish(CycleKind::CenterCircle, name)
}

fn build_rectangle(
    params: &ProbeParams,
    width_mm: f64,
    height_mm: f64,
    mode: EdgeMode,
) -> Result<crate::ProbePlan, ProbeError> {
    let center = require_center(params)?;
    let cx = center.get(Axis::X);
    let cy = center.get(Axis::Y);
    let cz = center.get(Axis::Z);
    let half_width = width_mm * 0.5;
    let half_height = height_mm * 0.5;
    let sides = [
        (ProbeAxis::X, true, cx + half_width, "x-plus"),
        (ProbeAxis::X, false, cx - half_width, "x-minus"),
        (ProbeAxis::Y, true, cy + half_height, "y-plus"),
        (ProbeAxis::Y, false, cy - half_height, "y-minus"),
    ];
    let mut builder = GcodeBuilder::new(params);
    for (axis, positive, wall, label) in sides {
        let (from, to) = rectangle_side(params, mode, axis, positive, wall, cx, cy, cz)?;
        builder.rapid_xy(from.get(Axis::X), from.get(Axis::Y))?;
        builder.rapid_z(cz)?;
        builder.probe(label, from, to)?;
        builder.retract()?;
    }
    builder.finish(
        CycleKind::CenterRectangle,
        format!("center-rectangle-{}", mode.as_str()),
    )
}

#[allow(clippy::too_many_arguments)]
fn rectangle_side(
    params: &ProbeParams,
    mode: EdgeMode,
    axis: ProbeAxis,
    positive: bool,
    wall_mm: f64,
    cx: f64,
    cy: f64,
    cz: f64,
) -> Result<(Position, Position), ProbeError> {
    let direction = match (mode, positive) {
        (EdgeMode::Inside, true) => Sign::Positive,
        (EdgeMode::Inside, false) => Sign::Negative,
        (EdgeMode::Outside, true) => Sign::Negative,
        (EdgeMode::Outside, false) => Sign::Positive,
    };
    let center_axis = match axis {
        ProbeAxis::X => cx,
        ProbeAxis::Y => cy,
    };
    let contact = wall_mm - direction.as_f64() * params.probe_radius_mm();
    let mut start = contact - direction.as_f64() * params.search_distance_mm;
    if mode == EdgeMode::Inside {
        start = if positive {
            start.max(center_axis)
        } else {
            start.min(center_axis)
        };
    }
    let (from, to) = match axis {
        ProbeAxis::X => (Position::xyz(start, cy, cz), Position::xyz(contact, cy, cz)),
        ProbeAxis::Y => (Position::xyz(cx, start, cz), Position::xyz(cx, contact, cz)),
    };
    Ok((from, to))
}

fn build_rotation(
    params: &ProbeParams,
    rotation: &RotationProbe,
) -> Result<crate::ProbePlan, ProbeError> {
    let base = require_position(params)?;
    let normal = rotation.normal_axis();
    let hint = rotation.contact_hint_mm(params);
    let start = rotation.start_mm(params);
    let z = base.get(Axis::Z);
    let p1 = normal.replace(base, start);
    let p1_target = normal.replace(base, hint);
    let shifted = match rotation.edge_axis {
        ProbeAxis::X => {
            let mut position = p1;
            position.set(Axis::X, base.get(Axis::X) + rotation.separation_mm);
            position
        }
        ProbeAxis::Y => {
            let mut position = p1;
            position.set(Axis::Y, base.get(Axis::Y) + rotation.separation_mm);
            position
        }
    };
    let p2_target = normal.replace(shifted, hint);

    let mut builder = GcodeBuilder::new(params);
    builder.rapid_xy(p1.get(Axis::X), p1.get(Axis::Y))?;
    builder.rapid_z(z)?;
    builder.probe("point-1", p1, p1_target)?;
    builder.retract()?;
    builder.rapid_xy(shifted.get(Axis::X), shifted.get(Axis::Y))?;
    builder.rapid_z(z)?;
    builder.probe("point-2", shifted, p2_target)?;
    builder.retract()?;
    builder.finish(CycleKind::Rotation, "rotation")
}
