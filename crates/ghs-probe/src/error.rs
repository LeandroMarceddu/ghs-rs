//! Typed validation and result errors.

use thiserror::Error;

/// A probe parameter or cycle geometry value that failed validation.
///
/// Values are millimetres or feed units as named by the field. The error keeps
/// the rejected value so the app can show it in the preflight sheet.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    /// A value was NaN or infinite.
    #[error("{field} must be finite, got {value}")]
    NonFinite {
        /// Field name.
        field: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// A value that must be greater than zero was zero or negative.
    #[error("{field} must be greater than zero, got {value}")]
    NotPositive {
        /// Field name.
        field: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// A value outside its inclusive bounds.
    #[error("{field} = {value} is outside [{min}, {max}]")]
    OutOfRange {
        /// Field name.
        field: &'static str,
        /// Rejected value.
        value: f64,
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
    },
    /// A cycle needs a parameter or geometry value that was absent.
    #[error("{0} is required for this cycle")]
    Missing(&'static str),
    /// A cycle specification used the wrong axis.
    #[error("{field} must use the {expected} axis, got {actual}")]
    WrongAxis {
        /// Field name.
        field: &'static str,
        /// Required axis letter.
        expected: &'static str,
        /// Supplied axis letter.
        actual: &'static str,
    },
    /// The safe Z plane is not above the start of a vertical search.
    #[error("safe Z {safe_z_mm} must be at or above the probe start {start_z_mm}")]
    SafeZTooLow {
        /// Configured safe Z in millimetres.
        safe_z_mm: f64,
        /// Computed search start in millimetres.
        start_z_mm: f64,
    },
    /// An XY probing height is above the safe Z plane.
    #[error("probing height {height_mm} must be at or below safe Z {safe_z_mm}")]
    ProbingHeightAboveSafeZ {
        /// Probing height in millimetres.
        height_mm: f64,
        /// Safe Z in millimetres.
        safe_z_mm: f64,
    },
    /// The probe diameter leaves no clearance for the feature.
    #[error("probe diameter {diameter_mm} mm leaves no clearance for {feature}")]
    ProbeTooLarge {
        /// Probe diameter in millimetres.
        diameter_mm: f64,
        /// Feature description.
        feature: &'static str,
    },
    /// The height-map grid exceeds the point budget.
    #[error("grid {columns} x {rows} exceeds the {max_points}-point limit")]
    GridTooLarge {
        /// Requested columns.
        columns: u32,
        /// Requested rows.
        rows: u32,
        /// Maximum number of points.
        max_points: usize,
    },
    /// The height-map travel from safe Z to the nominal surface is too long.
    #[error("height-map travel {travel_mm} mm exceeds search distance {search_mm} mm")]
    HeightMapTravel {
        /// Travel from safe Z to the nominal surface, in millimetres.
        travel_mm: f64,
        /// Configured search bound, in millimetres.
        search_mm: f64,
    },
}

/// A failed plan build or result computation.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ProbeError {
    /// A parameter or cycle value failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The probe result reported a missed or failed search.
    #[error("probe {index} reported failure")]
    ProbeFailed {
        /// Zero-based probe index in the cycle.
        index: usize,
    },
    /// A probe result carried a non-finite position.
    #[error("probe {index} reported a non-finite position")]
    NonFiniteProbe {
        /// Zero-based probe index in the cycle.
        index: usize,
    },
    /// The number of results does not match the cycle.
    #[error("expected {expected} probe results, got {actual}")]
    ResultCount {
        /// Expected count.
        expected: usize,
        /// Supplied count.
        actual: usize,
    },
    /// A geometry computation could not produce a unique answer.
    #[error("degenerate geometry: {0}")]
    Degenerate(&'static str),
    /// A computed value was not finite.
    #[error("computed value is not finite: {0}")]
    NonFinite(&'static str),
    /// The plan has no probe moves.
    #[error("a probe plan must contain at least one G38.2 move")]
    EmptyPlan,
    /// A probe move tried to start from an axis position that was never set.
    #[error("probe move starts from an unestablished {0} position")]
    UnestablishedPosition(&'static str),
}
