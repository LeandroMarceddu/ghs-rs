//! Pure result math for probe contacts.
//!
//! Contact coordinates are machine coordinates, matching grblHAL `PRB`
//! output. Every function validates finite input and returns a typed error for
//! degenerate geometry instead of panicking.

use crate::error::{ProbeError, ValidationError};
use crate::params::{
    EdgeMode, MAX_PLATE_THICKNESS_MM, ProbeAxis, Sign, XyPoint, check_finite, check_range,
};

/// Fitted circle through probe-centre contacts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircleFit {
    /// Fitted centre.
    pub center: XyPoint,
    /// Fitted radius of the contact points, in millimetres.
    pub radius_mm: f64,
    /// Root-mean-square radial error, in millimetres.
    pub rms_error_mm: f64,
}

/// Fitted rectangle dimensions from four measured edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectangleFit {
    /// Rectangle centre.
    pub center: XyPoint,
    /// Measured width, in millimetres.
    pub width_mm: f64,
    /// Measured height, in millimetres.
    pub height_mm: f64,
}

/// Rotation axis for a measured work rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RotationAxis {
    /// Rotation about the +Z axis, the only rotary axis in the XY probing
    /// plane.
    Z,
}

impl RotationAxis {
    /// Axis letter.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "Z"
    }
}

/// Measured edge angle in the XY plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationFit {
    /// Rotation axis.
    pub axis: RotationAxis,
    /// Angle from the +X axis, in radians in `(-pi, pi]`.
    pub angle_rad: f64,
    /// Angle from the +X axis, in degrees.
    pub angle_deg: f64,
    /// Nominal angle of the edge axis: `0` for an X edge, `90` for a Y edge.
    pub nominal_deg: f64,
    /// Shortest signed deviation from the nominal angle, in degrees in
    /// `(-90, 90]`.
    pub deviation_deg: f64,
}

/// Z work zero from a plate or surface contact.
///
/// `plate_thickness_mm` is subtracted so a contact on top of a plate reports
/// the work surface below it.
pub fn z_zero_from_contact(contact_z_mm: f64, plate_thickness_mm: f64) -> Result<f64, ProbeError> {
    check_finite("contact_z_mm", contact_z_mm)?;
    check_range(
        "plate_thickness_mm",
        plate_thickness_mm,
        0.0,
        MAX_PLATE_THICKNESS_MM,
    )?;
    let zero = contact_z_mm - plate_thickness_mm;
    check_finite("zero_z_mm", zero)?;
    Ok(zero)
}

/// Signed tool-length Z offset: `contact - reference`.
///
/// A tool that contacts above the reference surface (a shorter tool) reports a
/// positive offset; a longer tool reports a negative offset. The app chooses
/// the `G43.1`/work-offset sign convention after confirmation.
pub fn tool_length_offset(reference_z_mm: f64, contact_z_mm: f64) -> Result<f64, ProbeError> {
    check_finite("reference_z_mm", reference_z_mm)?;
    check_finite("contact_z_mm", contact_z_mm)?;
    let offset = contact_z_mm - reference_z_mm;
    check_finite("tool_length_offset_mm", offset)?;
    Ok(offset)
}

/// Edge coordinate from the probe-centre contact.
///
/// The touched surface is on the leading side of the centre, so
/// `edge = contact + sign * radius`.
pub fn edge_from_contact(
    contact_mm: f64,
    probe_radius_mm: f64,
    approach: Sign,
) -> Result<f64, ProbeError> {
    check_finite("contact_mm", contact_mm)?;
    check_range("probe_radius_mm", probe_radius_mm, 0.0, f64::MAX)?;
    let edge = contact_mm + approach.as_f64() * probe_radius_mm;
    check_finite("edge_mm", edge)?;
    Ok(edge)
}

/// Work zero from a measured edge, offset into the material.
///
/// `edge_offset_mm` is applied in the approach direction, which points into the
/// material for both an outside face and an inside pocket wall.
pub fn edge_work_zero(
    edge_mm: f64,
    approach: Sign,
    edge_offset_mm: f64,
) -> Result<f64, ProbeError> {
    check_finite("edge_mm", edge_mm)?;
    check_range("edge_offset_mm", edge_offset_mm, 0.0, f64::MAX)?;
    let zero = edge_mm + approach.as_f64() * edge_offset_mm;
    check_finite("edge_zero_mm", zero)?;
    Ok(zero)
}

/// Work zero from an X and a Y edge.
pub fn corner_work_zero(
    x_edge_mm: f64,
    y_edge_mm: f64,
    x_approach: Sign,
    y_approach: Sign,
    edge_offset_mm: f64,
) -> Result<XyPoint, ProbeError> {
    Ok(XyPoint::new(
        edge_work_zero(x_edge_mm, x_approach, edge_offset_mm)?,
        edge_work_zero(y_edge_mm, y_approach, edge_offset_mm)?,
    ))
}

/// Work zero from an X and a Y touch-plate edge.
///
/// This is the same correction as [`corner_work_zero`], named for the plate
/// workflow.
pub fn xy_plate_zero(
    x_edge_mm: f64,
    y_edge_mm: f64,
    x_approach: Sign,
    y_approach: Sign,
    edge_offset_mm: f64,
) -> Result<XyPoint, ProbeError> {
    corner_work_zero(x_edge_mm, y_edge_mm, x_approach, y_approach, edge_offset_mm)
}

/// Fit a circle to three or more contact points.
///
/// Uses an algebraic least-squares fit (Kasa). The centre is independent of the
/// probe radius when every touch is on the same feature, because all contact
/// centres lie on a concentric circle. The fitted radius is the contact-centre
/// radius; use [`circle_feature_radius`] for the workpiece radius.
pub fn fit_circle(points: &[XyPoint]) -> Result<CircleFit, ProbeError> {
    if points.len() < 3 {
        return Err(ProbeError::Degenerate(
            "circle fit needs at least three points",
        ));
    }
    let mut mean_x = 0.0;
    let mut mean_y = 0.0;
    for point in points {
        if !point.is_finite() {
            return Err(ProbeError::NonFinite("circle fit point"));
        }
        mean_x += point.x_mm;
        mean_y += point.y_mm;
    }
    let count = points.len() as f64;
    mean_x /= count;
    mean_y /= count;

    let mut sxx = 0.0;
    let mut sxy = 0.0;
    let mut syy = 0.0;
    let mut sxu = 0.0;
    let mut syu = 0.0;
    let mut mean_u = 0.0;
    for point in points {
        let u = point.x_mm * point.x_mm + point.y_mm * point.y_mm;
        mean_u += u;
    }
    mean_u /= count;
    for point in points {
        let dx = point.x_mm - mean_x;
        let dy = point.y_mm - mean_y;
        let u = point.x_mm * point.x_mm + point.y_mm * point.y_mm;
        let du = u - mean_u;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
        sxu += dx * du;
        syu += dy * du;
    }

    let determinant = sxx * syy - sxy * sxy;
    let scale = (sxx * syy).abs();
    if !determinant.is_finite() || determinant.abs() <= 1e-12 * scale {
        return Err(ProbeError::Degenerate(
            "circle fit points are collinear or coincident",
        ));
    }
    let a = (sxu * syy - syu * sxy) / determinant;
    let b = (syu * sxx - sxu * sxy) / determinant;
    // The centred model is `u' = 2*cx*x' + 2*cy*y'`, so the solved
    // coefficients are the actual centre coordinates, not mean-relative ones.
    let center = XyPoint::new(a * 0.5, b * 0.5);
    if !center.is_finite() {
        return Err(ProbeError::NonFinite("circle centre"));
    }

    let mut radius = 0.0;
    for point in points {
        radius += point.distance(center);
    }
    radius /= count;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(ProbeError::Degenerate("circle radius is not positive"));
    }
    let mut squared_error = 0.0;
    for point in points {
        let error = point.distance(center) - radius;
        squared_error += error * error;
    }
    let rms_error_mm = (squared_error / count).sqrt();
    if !rms_error_mm.is_finite() {
        return Err(ProbeError::NonFinite("circle fit error"));
    }
    Ok(CircleFit {
        center,
        radius_mm: radius,
        rms_error_mm,
    })
}

/// Correct a fitted contact radius to the workpiece feature radius.
///
/// Inside a bore the contact centres sit one probe radius nearer the centre, so
/// the feature radius is larger; on a boss they sit one radius outside, so the
/// feature radius is smaller.
pub fn circle_feature_radius(
    contact_radius_mm: f64,
    probe_radius_mm: f64,
    mode: EdgeMode,
) -> Result<f64, ProbeError> {
    check_finite("contact_radius_mm", contact_radius_mm)?;
    check_range("probe_radius_mm", probe_radius_mm, 0.0, f64::MAX)?;
    let radius = match mode {
        EdgeMode::Inside => contact_radius_mm + probe_radius_mm,
        EdgeMode::Outside => contact_radius_mm - probe_radius_mm,
    };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(ProbeError::Validation(ValidationError::ProbeTooLarge {
            diameter_mm: probe_radius_mm * 2.0,
            feature: "circle radius",
        }));
    }
    Ok(radius)
}

/// Rectangle centre and measured size from the four edge coordinates.
///
/// The negative and positive edges must be ordered correctly; a crossed pair
/// means the probe hit the wrong feature and is rejected.
pub fn rectangle_center_from_edges(
    x_neg_mm: f64,
    x_pos_mm: f64,
    y_neg_mm: f64,
    y_pos_mm: f64,
) -> Result<RectangleFit, ProbeError> {
    for (field, value) in [
        ("x_neg_mm", x_neg_mm),
        ("x_pos_mm", x_pos_mm),
        ("y_neg_mm", y_neg_mm),
        ("y_pos_mm", y_pos_mm),
    ] {
        check_finite(field, value)?;
    }
    let width_mm = x_pos_mm - x_neg_mm;
    let height_mm = y_pos_mm - y_neg_mm;
    if !width_mm.is_finite() || !height_mm.is_finite() {
        return Err(ProbeError::NonFinite("rectangle size"));
    }
    if width_mm <= 0.0 {
        return Err(ProbeError::Degenerate("rectangle X edges are not ordered"));
    }
    if height_mm <= 0.0 {
        return Err(ProbeError::Degenerate("rectangle Y edges are not ordered"));
    }
    let center = XyPoint::new(
        x_neg_mm * 0.5 + x_pos_mm * 0.5,
        y_neg_mm * 0.5 + y_pos_mm * 0.5,
    );
    if !center.is_finite() {
        return Err(ProbeError::NonFinite("rectangle centre"));
    }
    Ok(RectangleFit {
        center,
        width_mm,
        height_mm,
    })
}

/// Rectangle centre from four symmetric touches.
///
/// The four points must be the rectangle's four corners, or the midpoints of
/// its four sides. Symmetric probe-radius offsets cancel, so the centre is the
/// average. This function reports the centre only; use the four side probes for
/// measured width and height.
pub fn rectangle_center_from_touches(points: [XyPoint; 4]) -> Result<XyPoint, ProbeError> {
    let mut x_mm = 0.0;
    let mut y_mm = 0.0;
    for point in points {
        if !point.is_finite() {
            return Err(ProbeError::NonFinite("rectangle touch"));
        }
        x_mm += point.x_mm * 0.25;
        y_mm += point.y_mm * 0.25;
    }
    let center = XyPoint::new(x_mm, y_mm);
    if !center.is_finite() {
        return Err(ProbeError::NonFinite("rectangle centre"));
    }
    Ok(center)
}

/// Edge angle from two points.
///
/// `angle_deg` is measured from +X and is positive counter-clockwise about +Z.
/// `nominal_deg` is derived from the edge axis and `deviation_deg` is the
/// shortest signed difference, so an edge nominally along Y reports near zero
/// deviation at 90 degrees.
pub fn rotation_from_points(
    p1: XyPoint,
    p2: XyPoint,
    edge_axis: ProbeAxis,
) -> Result<RotationFit, ProbeError> {
    if !p1.is_finite() || !p2.is_finite() {
        return Err(ProbeError::NonFinite("rotation point"));
    }
    if p1.distance(p2) <= 1e-9 {
        return Err(ProbeError::Degenerate("rotation points must be separated"));
    }
    let angle_rad = (p2.y_mm - p1.y_mm).atan2(p2.x_mm - p1.x_mm);
    let angle_deg = angle_rad.to_degrees();
    if !angle_rad.is_finite() || !angle_deg.is_finite() {
        return Err(ProbeError::NonFinite("rotation angle"));
    }
    let nominal_deg = match edge_axis {
        ProbeAxis::X => 0.0,
        ProbeAxis::Y => 90.0,
    };
    let deviation_deg = shortest_line_deviation_deg(angle_deg - nominal_deg);
    Ok(RotationFit {
        axis: RotationAxis::Z,
        angle_rad,
        angle_deg,
        nominal_deg,
        deviation_deg,
    })
}

/// Shortest signed deviation between two unoriented lines, in degrees.
fn shortest_line_deviation_deg(delta_deg: f64) -> f64 {
    let wrapped = (delta_deg + 90.0).rem_euclid(180.0) - 90.0;
    if wrapped <= -90.0 {
        wrapped + 180.0
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_fit_finds_a_known_centre() {
        let points = [
            XyPoint::new(14.0, 20.0),
            XyPoint::new(8.0, 16.535_898_384_862_246),
            XyPoint::new(8.0, 23.464_101_615_137_754),
        ];
        let fit = fit_circle(&points).expect("valid circle");
        assert!((fit.center.x_mm - 10.0).abs() < 1e-9);
        assert!((fit.center.y_mm - 20.0).abs() < 1e-9);
        assert!((fit.radius_mm - 4.0).abs() < 1e-9);
        assert!(fit.rms_error_mm < 1e-9);
    }

    #[test]
    fn circle_fit_rejects_collinear_points() {
        let points = [
            XyPoint::new(0.0, 0.0),
            XyPoint::new(1.0, 0.0),
            XyPoint::new(2.0, 0.0),
        ];
        assert!(matches!(
            fit_circle(&points),
            Err(ProbeError::Degenerate(_))
        ));
        assert!(matches!(
            fit_circle(&points[..2]),
            Err(ProbeError::Degenerate(_))
        ));
    }

    #[test]
    fn rotation_uses_the_shortest_line_deviation() {
        let fit = rotation_from_points(
            XyPoint::new(0.0, 0.0),
            XyPoint::new(10.0, 1.0),
            ProbeAxis::X,
        )
        .expect("valid");
        assert!((fit.deviation_deg - 5.710_593_137_499_642).abs() < 1e-9);

        let flipped = rotation_from_points(
            XyPoint::new(0.0, 0.0),
            XyPoint::new(-10.0, 0.0),
            ProbeAxis::X,
        )
        .expect("valid");
        assert!(flipped.deviation_deg.abs() < 1e-9);
    }
}
