//! Cycle results and the correlation-friendly outcome type.

use ghs_core::{Axis, ProbeResult, Units, Wcs};

use crate::cycle::{CornerProbe, ProbeCycle, RotationProbe};
use crate::error::ProbeError;
use crate::math::{
    RectangleFit, RotationAxis, RotationFit, circle_feature_radius, corner_work_zero,
    rectangle_center_from_edges, rotation_from_points, tool_length_offset, z_zero_from_contact,
};
use crate::params::{EdgeMode, ProbeAxis, ProbeParams, Sign, XyPoint};

/// Kind of a probe cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CycleKind {
    /// Z touch plate.
    ZTouchPlate,
    /// Tool-length reference.
    ToolLength,
    /// Single edge.
    Edge,
    /// Two-edge corner.
    Corner,
    /// Circle centre.
    CenterCircle,
    /// Rectangle centre.
    CenterRectangle,
    /// Two-point rotation.
    Rotation,
    /// XY touch plate.
    XyTouchPlate,
    /// Height map.
    HeightMap,
}

impl CycleKind {
    /// Stable lower-case name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CycleKind::ZTouchPlate => "z-touch-plate",
            CycleKind::ToolLength => "tool-length",
            CycleKind::Edge => "edge",
            CycleKind::Corner => "corner",
            CycleKind::CenterCircle => "center-circle",
            CycleKind::CenterRectangle => "center-rectangle",
            CycleKind::Rotation => "rotation",
            CycleKind::XyTouchPlate => "xy-touch-plate",
            CycleKind::HeightMap => "height-map",
        }
    }
}

/// One raw probe result with its position in the cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSample {
    /// Zero-based index in the cycle.
    pub index: usize,
    /// Stable label matching the plan's [`crate::PlannedProbe::label`] at the
    /// same index.
    pub label: String,
    /// Raw controller result, in machine coordinates.
    pub result: ProbeResult,
}

/// Computed value of a probe cycle.
///
/// Every position and zero is in machine coordinates, matching the raw `PRB`
/// results. A delta such as the tool-length offset or a rotation angle is
/// frame-independent.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeValue {
    /// Z work zero from a plate contact.
    ZZero {
        /// Z of the probe-centre contact, in machine millimetres.
        contact_z_mm: f64,
        /// Plate thickness subtracted from the contact, in millimetres.
        plate_thickness_mm: f64,
        /// Resulting Z work zero, in machine millimetres.
        zero_z_mm: f64,
    },
    /// Tool-length Z offset.
    ToolLength {
        /// Reference surface Z, in machine millimetres.
        reference_z_mm: f64,
        /// Z of the probe-centre contact, in machine millimetres.
        contact_z_mm: f64,
        /// `contact - reference`, in millimetres. Signed delta to add to the
        /// stored tool-length offset; positive for a shorter tool.
        offset_mm: f64,
    },
    /// One measured edge and its work zero.
    Edge {
        /// Probed axis.
        axis: ProbeAxis,
        /// Physical side of the measurement.
        mode: EdgeMode,
        /// Measured edge coordinate, in machine millimetres.
        edge_mm: f64,
        /// Work zero coordinate, in machine millimetres.
        zero_mm: f64,
    },
    /// Corner from two edges.
    Corner {
        /// Measured X edge, in machine millimetres.
        x_edge_mm: f64,
        /// Measured Y edge, in machine millimetres.
        y_edge_mm: f64,
        /// Work zero at the offset corner, in machine millimetres.
        zero: XyPoint,
    },
    /// Circle centre from three or more touches.
    Center {
        /// Fitted centre, in machine millimetres.
        center: XyPoint,
        /// Fitted radius of the probe-centre contacts, in millimetres.
        contact_radius_mm: f64,
        /// Workpiece radius after probe-radius correction, in millimetres.
        feature_radius_mm: f64,
        /// Root-mean-square radial error of the fit, in millimetres.
        rms_error_mm: f64,
    },
    /// Rectangle centre from four side touches.
    RectangleCenter {
        /// Fitted centre, in machine millimetres.
        center: XyPoint,
        /// Measured width, in millimetres.
        width_mm: f64,
        /// Measured height, in millimetres.
        height_mm: f64,
    },
    /// Rotation measured from two edge points.
    Rotation {
        /// Rotation axis.
        axis: RotationAxis,
        /// First edge point, in machine millimetres.
        p1: XyPoint,
        /// Second edge point, in machine millimetres.
        p2: XyPoint,
        /// Angle from +X, in radians.
        angle_rad: f64,
        /// Angle from +X, in degrees.
        angle_deg: f64,
        /// Nominal angle of the edge axis, in degrees.
        nominal_deg: f64,
        /// Shortest signed deviation from nominal, in degrees.
        deviation_deg: f64,
    },
    /// XY touch-plate zero.
    XyZero {
        /// Measured X edge, in machine millimetres.
        x_edge_mm: f64,
        /// Measured Y edge, in machine millimetres.
        y_edge_mm: f64,
        /// Work zero at the offset plate corner, in machine millimetres.
        zero: XyPoint,
    },
}

/// Computed cycle result with every raw probe position.
///
/// `samples` keeps the controller results in machine coordinates for audit;
/// `value` is derived from them without changing the frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeOutcome {
    /// Cycle kind.
    pub cycle: CycleKind,
    /// Work coordinate system selected by the plan.
    pub wcs: Wcs,
    /// Display units requested by the operator.
    pub units: Units,
    /// Raw results, in cycle order.
    pub samples: Vec<ProbeSample>,
    /// Computed value.
    pub value: ProbeValue,
}

impl ProbeOutcome {
    /// Correlate ordered `PRB` results with a cycle and compute its value.
    ///
    /// The app binds the Nth [`ghs_core::ProbeResult`] from the engine to the
    /// probe at index N in the plan. A failed or non-finite result is rejected
    /// instead of producing a partial value.
    pub fn from_results(
        cycle: &ProbeCycle,
        params: &ProbeParams,
        results: &[ProbeResult],
    ) -> Result<Self, ProbeError> {
        cycle.validate(params)?;
        let expected = cycle.expected_probes();
        if results.len() != expected {
            return Err(ProbeError::ResultCount {
                expected,
                actual: results.len(),
            });
        }
        let labels = cycle.labels();
        let mut samples = Vec::with_capacity(expected);
        for (index, result) in results.iter().enumerate() {
            if !result.success {
                return Err(ProbeError::ProbeFailed { index });
            }
            if !result.position.is_finite() {
                return Err(ProbeError::NonFiniteProbe { index });
            }
            samples.push(ProbeSample {
                index,
                label: labels
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("probe-{index}")),
                result: *result,
            });
        }
        let value = compute_value(cycle, params, results)?;
        Ok(ProbeOutcome {
            cycle: cycle.kind(),
            wcs: params.wcs,
            units: params.units,
            samples,
            value,
        })
    }

    /// Raw probe positions in cycle order.
    pub fn raw_positions(&self) -> impl Iterator<Item = ghs_core::Position> + '_ {
        self.samples.iter().map(|sample| sample.result.position)
    }
}

fn compute_value(
    cycle: &ProbeCycle,
    params: &ProbeParams,
    results: &[ProbeResult],
) -> Result<ProbeValue, ProbeError> {
    match cycle {
        ProbeCycle::ZTouchPlate {
            plate_thickness_mm, ..
        } => {
            let contact_z_mm = results[0].position.get(Axis::Z);
            let zero_z_mm = z_zero_from_contact(contact_z_mm, *plate_thickness_mm)?;
            Ok(ProbeValue::ZZero {
                contact_z_mm,
                plate_thickness_mm: *plate_thickness_mm,
                zero_z_mm,
            })
        }
        ProbeCycle::ToolLength { reference_z_mm } => {
            let contact_z_mm = results[0].position.get(Axis::Z);
            let offset_mm = tool_length_offset(*reference_z_mm, contact_z_mm)?;
            Ok(ProbeValue::ToolLength {
                reference_z_mm: *reference_z_mm,
                contact_z_mm,
                offset_mm,
            })
        }
        ProbeCycle::Edge(spec) => {
            let contact_mm = spec.axis.component(results[0].position);
            let edge_mm = spec.edge_mm(params, contact_mm)?;
            let zero_mm = spec.zero_mm(params, contact_mm)?;
            Ok(ProbeValue::Edge {
                axis: spec.axis,
                mode: spec.mode,
                edge_mm,
                zero_mm,
            })
        }
        ProbeCycle::Corner(corner) => {
            let (x_edge_mm, y_edge_mm, zero) = corner_values(corner, params, results)?;
            Ok(ProbeValue::Corner {
                x_edge_mm,
                y_edge_mm,
                zero,
            })
        }
        ProbeCycle::XyTouchPlate(corner) => {
            let (x_edge_mm, y_edge_mm, zero) = corner_values(corner, params, results)?;
            Ok(ProbeValue::XyZero {
                x_edge_mm,
                y_edge_mm,
                zero,
            })
        }
        ProbeCycle::CenterCircle { mode, .. } => {
            let points: Vec<XyPoint> = results
                .iter()
                .map(|result| XyPoint::from_position(result.position))
                .collect();
            let fit = crate::math::fit_circle(&points)?;
            let feature_radius_mm =
                circle_feature_radius(fit.radius_mm, params.probe_radius_mm(), *mode)?;
            Ok(ProbeValue::Center {
                center: fit.center,
                contact_radius_mm: fit.radius_mm,
                feature_radius_mm,
                rms_error_mm: fit.rms_error_mm,
            })
        }
        ProbeCycle::CenterRectangle { mode, .. } => {
            let fit = rectangle_values(*mode, params, results)?;
            Ok(ProbeValue::RectangleCenter {
                center: fit.center,
                width_mm: fit.width_mm,
                height_mm: fit.height_mm,
            })
        }
        ProbeCycle::Rotation(rotation) => {
            let (fit, p1, p2) = rotation_values(rotation, params, results)?;
            Ok(ProbeValue::Rotation {
                axis: fit.axis,
                p1,
                p2,
                angle_rad: fit.angle_rad,
                angle_deg: fit.angle_deg,
                nominal_deg: fit.nominal_deg,
                deviation_deg: fit.deviation_deg,
            })
        }
    }
}

fn corner_values(
    corner: &CornerProbe,
    params: &ProbeParams,
    results: &[ProbeResult],
) -> Result<(f64, f64, XyPoint), ProbeError> {
    let x_contact = corner.x.axis.component(results[0].position);
    let y_contact = corner.y.axis.component(results[1].position);
    let x_edge_mm = corner.x.edge_mm(params, x_contact)?;
    let y_edge_mm = corner.y.edge_mm(params, y_contact)?;
    let zero = corner_work_zero(
        x_edge_mm,
        y_edge_mm,
        corner.x.approach,
        corner.y.approach,
        params.edge_offset_mm,
    )?;
    Ok((x_edge_mm, y_edge_mm, zero))
}

fn rectangle_values(
    mode: EdgeMode,
    params: &ProbeParams,
    results: &[ProbeResult],
) -> Result<RectangleFit, ProbeError> {
    let radius = params.probe_radius_mm();
    let contacts = [
        results[0].position.get(Axis::X),
        results[1].position.get(Axis::X),
        results[2].position.get(Axis::Y),
        results[3].position.get(Axis::Y),
    ];
    let directions = rectangle_directions(mode);
    let edges = [
        contacts[0] + directions[0].as_f64() * radius,
        contacts[1] + directions[1].as_f64() * radius,
        contacts[2] + directions[2].as_f64() * radius,
        contacts[3] + directions[3].as_f64() * radius,
    ];
    rectangle_center_from_edges(edges[1], edges[0], edges[3], edges[2])
}

fn rectangle_directions(mode: EdgeMode) -> [Sign; 4] {
    match mode {
        EdgeMode::Inside => [
            Sign::Positive,
            Sign::Negative,
            Sign::Positive,
            Sign::Negative,
        ],
        EdgeMode::Outside => [
            Sign::Negative,
            Sign::Positive,
            Sign::Negative,
            Sign::Positive,
        ],
    }
}

fn rotation_values(
    rotation: &RotationProbe,
    params: &ProbeParams,
    results: &[ProbeResult],
) -> Result<(RotationFit, XyPoint, XyPoint), ProbeError> {
    let radius = params.probe_radius_mm();
    let sign = rotation.approach.as_f64();
    let normal = rotation.edge_axis.other();
    let p1 = edge_point(results[0].position, normal, sign * radius);
    let p2 = edge_point(results[1].position, normal, sign * radius);
    let fit = rotation_from_points(p1, p2, rotation.edge_axis)?;
    Ok((fit, p1, p2))
}

fn edge_point(position: ghs_core::Position, axis: ProbeAxis, delta: f64) -> XyPoint {
    let mut point = XyPoint::from_position(position);
    match axis {
        ProbeAxis::X => point.x_mm += delta,
        ProbeAxis::Y => point.y_mm += delta,
    }
    point
}
