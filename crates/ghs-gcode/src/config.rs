//! Parser configuration: known controller offsets, limits, and tolerances.

use ghs_core::Position;
use ghs_viewer::DEFAULT_CHORD_TOLERANCE;

/// Largest absolute coordinate accepted, in millimetres.
///
/// This is far beyond any real machine envelope but keeps the `f64` geometry
/// well inside the viewer's `f32` range.
pub const MAX_COORDINATE_MM: f64 = 1.0e7;

/// Limits on representative arc geometry.
pub const MIN_ARC_RADIUS_MM: f64 = 1.0e-6;

/// Configuration for [`Parser`](crate::Parser) and
/// [`parse_program_with`](crate::parse_program_with).
///
/// The defaults describe a machine at G54 with zero offsets, which makes the
/// toolpath equal to the program coordinates. Callers that have the
/// controller's `$#` offsets should pass them so the viewer can overlay the
/// path on the machine's work envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct ParserConfig {
    /// Work coordinate offsets in `G54`..`G59.3` order, in millimetres.
    ///
    /// The parser starts at program position `(0, 0, 0)`, so the initial
    /// machine position is the offset of the active work coordinate system.
    pub offsets: [Position; 9],
    /// Stored `G28` position in machine coordinates, in millimetres.
    pub home_position: Option<[f64; 3]>,
    /// Stored `G30` position in machine coordinates, in millimetres.
    pub second_home_position: Option<[f64; 3]>,
    /// Maximum arc sagitta for display tessellation, in millimetres.
    pub chord_tolerance_mm: f64,
    /// Minimum allowed start/end radius mismatch for an arc, in millimetres.
    ///
    /// grblHAL accepts a mismatch of up to `max(this, 0.1% of radius)`.
    pub arc_radius_tolerance_mm: f64,
    /// Maximum source line length in bytes.
    pub max_line_bytes: usize,
    /// Maximum words on one line.
    pub max_words: usize,
    /// Maximum errors collected before parsing stops.
    pub max_errors: usize,
    /// Maximum Z step between helical-arc display slices, in millimetres.
    pub max_helix_step_mm: f64,
    /// Maximum display slices generated for one helical arc.
    pub max_helix_slices: usize,
    /// Clearance above the previous peck used by `G73`/`G83`, in millimetres.
    pub peck_clearance_mm: f64,
    /// Largest accepted absolute coordinate, in millimetres.
    pub max_coordinate_mm: f64,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            offsets: [Position::zero(); 9],
            home_position: None,
            second_home_position: None,
            chord_tolerance_mm: f64::from(DEFAULT_CHORD_TOLERANCE),
            arc_radius_tolerance_mm: 0.005,
            max_line_bytes: 4096,
            max_words: 256,
            max_errors: 100,
            max_helix_step_mm: 0.5,
            max_helix_slices: 256,
            peck_clearance_mm: 0.5,
            max_coordinate_mm: MAX_COORDINATE_MM,
        }
    }
}

impl ParserConfig {
    /// Sets the work coordinate offsets.
    #[must_use]
    pub fn with_offsets(mut self, offsets: [Position; 9]) -> Self {
        self.offsets = offsets;
        self
    }

    /// Sets the stored `G28` position, in machine millimetres.
    #[must_use]
    pub fn with_home_position(mut self, position: [f64; 3]) -> Self {
        self.home_position = Some(position);
        self
    }

    /// Sets the stored `G30` position, in machine millimetres.
    #[must_use]
    pub fn with_second_home_position(mut self, position: [f64; 3]) -> Self {
        self.second_home_position = Some(position);
        self
    }

    /// Sets the display chord tolerance in millimetres.
    #[must_use]
    pub fn with_chord_tolerance_mm(mut self, tolerance_mm: f64) -> Self {
        self.chord_tolerance_mm = tolerance_mm;
        self
    }

    /// Replaces degenerate values with safe defaults.
    pub(crate) fn normalized(mut self) -> Self {
        for offset in &mut self.offsets {
            for axis in [ghs_core::Axis::X, ghs_core::Axis::Y, ghs_core::Axis::Z] {
                let value = offset.get(axis);
                if !value.is_finite() || value.abs() > MAX_COORDINATE_MM {
                    tracing::warn!(?axis, value, "ignoring invalid work offset");
                    offset.set(axis, 0.0);
                }
            }
        }
        for home in [&mut self.home_position, &mut self.second_home_position] {
            if let Some(position) = home
                && position
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > MAX_COORDINATE_MM)
            {
                tracing::warn!("ignoring invalid stored home position");
                *home = None;
            }
        }
        if !self.chord_tolerance_mm.is_finite() || self.chord_tolerance_mm <= 0.0 {
            self.chord_tolerance_mm = f64::from(DEFAULT_CHORD_TOLERANCE);
        }
        if !self.arc_radius_tolerance_mm.is_finite() || self.arc_radius_tolerance_mm < 0.0 {
            self.arc_radius_tolerance_mm = 0.005;
        }
        if !self.max_helix_step_mm.is_finite() || self.max_helix_step_mm <= 0.0 {
            self.max_helix_step_mm = 0.5;
        }
        if !self.peck_clearance_mm.is_finite() || self.peck_clearance_mm < 0.0 {
            self.peck_clearance_mm = 0.5;
        }
        if !self.max_coordinate_mm.is_finite() || self.max_coordinate_mm <= 0.0 {
            self.max_coordinate_mm = MAX_COORDINATE_MM;
        }
        self.max_words = self.max_words.max(1);
        self.max_line_bytes = self.max_line_bytes.max(16);
        self.max_errors = self.max_errors.max(1);
        self.max_helix_slices = self.max_helix_slices.max(1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_core::Axis;

    #[test]
    fn defaults_are_sane() {
        let config = ParserConfig::default();
        assert_eq!(config.offsets, [Position::zero(); 9]);
        assert!(config.chord_tolerance_mm > 0.0);
        assert!(config.max_line_bytes >= 256);
    }

    #[test]
    fn normalization_rejects_degenerate_limits() {
        let config = ParserConfig {
            chord_tolerance_mm: 0.0,
            arc_radius_tolerance_mm: f64::NAN,
            max_helix_step_mm: -1.0,
            max_words: 0,
            max_line_bytes: 0,
            max_errors: 0,
            max_helix_slices: 0,
            peck_clearance_mm: -1.0,
            max_coordinate_mm: f64::INFINITY,
            ..ParserConfig::default()
        }
        .normalized();
        assert!(config.chord_tolerance_mm > 0.0);
        assert_eq!(config.arc_radius_tolerance_mm, 0.005);
        assert_eq!(config.max_helix_step_mm, 0.5);
        assert_eq!(config.max_words, 1);
        assert_eq!(config.max_line_bytes, 16);
        assert_eq!(config.max_errors, 1);
        assert_eq!(config.max_helix_slices, 1);
        assert_eq!(config.max_coordinate_mm, MAX_COORDINATE_MM);
    }

    #[test]
    fn invalid_offsets_are_zeroed() {
        let mut offsets = [Position::zero(); 9];
        offsets[0].set(Axis::X, f64::NAN);
        offsets[0].set(Axis::Y, f64::INFINITY);
        let config = ParserConfig {
            offsets,
            home_position: Some([f64::NAN, 0.0, 0.0]),
            ..ParserConfig::default()
        }
        .normalized();
        assert_eq!(config.offsets[0].get(Axis::X), 0.0);
        assert_eq!(config.offsets[0].get(Axis::Y), 0.0);
        assert_eq!(config.home_position, None);
    }
}
