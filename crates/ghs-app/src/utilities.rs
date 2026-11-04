//! Utility models: material surfacing, program limits, and source transforms.
//!
//! Everything here is pure data and text handling. The models never send a
//! controller command, and the surfacing/transform generators only produce
//! `LoadProgram` payloads; starting a job stays a separate, explicit action.
//!
//! The transform functions are non-destructive: they return a new source
//! string and leave the loaded program untouched. When the app loads the
//! transformed source, any height-map Apply/Revert state for the previous
//! source is cleared, because the height-map model tracks the current part
//! program. Keep an external copy if the pre-transform original matters.

use ghs_core::Wcs;
use ghs_engine::Program;

/// Upper bound for one generated surfacing program.
pub const MAX_SURFACING_LINES: usize = 10_000;
/// Upper bound for the number of depth passes.
pub const MAX_SURFACING_DEPTH_PASSES: usize = 1_000;
/// Upper bound for the number of raster rows per depth pass.
pub const MAX_SURFACING_ROWS: usize = 5_000;
/// Largest accepted absolute work-coordinate origin, in millimetres.
pub const MAX_ORIGIN_MM: f64 = 1_000_000.0;
/// Largest accepted transformed source size, in bytes.
pub const MAX_TRANSFORM_BYTES: usize = 20 * 1024 * 1024;

/// Height-map wording shown in the transform preview.
pub const TRANSFORM_HEIGHT_MAP_NOTE: &str = "LOAD AS PROGRAM replaces the current job source. Any \
     height-map Apply/Revert state for the previous source is cleared; a new map run adopts the \
     transformed source. Keep an external copy if you need the pre-transform original.";

/// Program name suffix for a line-numbered transform.
pub const LINE_NUMBER_SUFFIX: &str = " [line numbers]";
/// Program name suffix for a comment-stripped transform.
pub const STRIPPED_COMMENT_SUFFIX: &str = " [comments stripped]";

// ---------------------------------------------------------------- surfacing

/// One editable surfacing wizard field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UtilityField {
    /// Stock origin X in work coordinates, in millimetres.
    OriginX,
    /// Stock origin Y in work coordinates, in millimetres.
    OriginY,
    /// Stock extent along X, in millimetres.
    StockWidth,
    /// Stock extent along Y, in millimetres.
    StockDepth,
    /// Cutter diameter, in millimetres.
    CutterDiameter,
    /// Stepover as a percentage of the cutter diameter.
    StepoverPercent,
    /// Total material to remove, in millimetres.
    TotalDepth,
    /// Maximum depth removed by one pass, in millimetres.
    DepthPerPass,
    /// Cutting feed, in millimetres per minute.
    Feed,
    /// Plunge feed, in millimetres per minute.
    PlungeFeed,
    /// Retract plane in work coordinates, in millimetres.
    SafeZ,
    /// Spindle speed, in revolutions per minute.
    SpindleRpm,
    /// First line number for the numbering transform.
    LineStart,
    /// Increment for the numbering transform.
    LineStep,
}

impl UtilityField {
    /// Number of editable fields.
    pub const COUNT: usize = 14;

    /// Every field, in edit order.
    pub const ALL: [UtilityField; UtilityField::COUNT] = [
        UtilityField::OriginX,
        UtilityField::OriginY,
        UtilityField::StockWidth,
        UtilityField::StockDepth,
        UtilityField::CutterDiameter,
        UtilityField::StepoverPercent,
        UtilityField::TotalDepth,
        UtilityField::DepthPerPass,
        UtilityField::Feed,
        UtilityField::PlungeFeed,
        UtilityField::SafeZ,
        UtilityField::SpindleRpm,
        UtilityField::LineStart,
        UtilityField::LineStep,
    ];

    /// Default text for every field.
    pub const DEFAULTS: [&'static str; UtilityField::COUNT] = [
        "0", "0", "100", "80", "6", "40", "2", "1", "900", "200", "5", "12000", "10", "10",
    ];

    /// Stable zero-based index for the field array.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            UtilityField::OriginX => 0,
            UtilityField::OriginY => 1,
            UtilityField::StockWidth => 2,
            UtilityField::StockDepth => 3,
            UtilityField::CutterDiameter => 4,
            UtilityField::StepoverPercent => 5,
            UtilityField::TotalDepth => 6,
            UtilityField::DepthPerPass => 7,
            UtilityField::Feed => 8,
            UtilityField::PlungeFeed => 9,
            UtilityField::SafeZ => 10,
            UtilityField::SpindleRpm => 11,
            UtilityField::LineStart => 12,
            UtilityField::LineStep => 13,
        }
    }

    /// Upper-case label for the field cell.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            UtilityField::OriginX => "ORIGIN X WORK",
            UtilityField::OriginY => "ORIGIN Y WORK",
            UtilityField::StockWidth => "STOCK WIDTH",
            UtilityField::StockDepth => "STOCK DEPTH",
            UtilityField::CutterDiameter => "CUTTER DIA",
            UtilityField::StepoverPercent => "STEPOVER",
            UtilityField::TotalDepth => "TOTAL DEPTH",
            UtilityField::DepthPerPass => "DEPTH/PASS",
            UtilityField::Feed => "FEED",
            UtilityField::PlungeFeed => "PLUNGE FEED",
            UtilityField::SafeZ => "SAFE Z",
            UtilityField::SpindleRpm => "SPINDLE RPM",
            UtilityField::LineStart => "LINE START",
            UtilityField::LineStep => "LINE STEP",
        }
    }

    /// Unit label, or an empty string for unitless fields.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            UtilityField::OriginX
            | UtilityField::OriginY
            | UtilityField::StockWidth
            | UtilityField::StockDepth
            | UtilityField::CutterDiameter
            | UtilityField::TotalDepth
            | UtilityField::DepthPerPass
            | UtilityField::SafeZ => "mm",
            UtilityField::StepoverPercent => "%",
            UtilityField::Feed | UtilityField::PlungeFeed => "mm/min",
            UtilityField::SpindleRpm => "rpm",
            UtilityField::LineStart | UtilityField::LineStep => "",
        }
    }

    /// Stable short key for element ids.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            UtilityField::OriginX => "origin-x",
            UtilityField::OriginY => "origin-y",
            UtilityField::StockWidth => "stock-width",
            UtilityField::StockDepth => "stock-depth",
            UtilityField::CutterDiameter => "cutter-diameter",
            UtilityField::StepoverPercent => "stepover",
            UtilityField::TotalDepth => "total-depth",
            UtilityField::DepthPerPass => "depth-per-pass",
            UtilityField::Feed => "feed",
            UtilityField::PlungeFeed => "plunge-feed",
            UtilityField::SafeZ => "safe-z",
            UtilityField::SpindleRpm => "spindle-rpm",
            UtilityField::LineStart => "line-start",
            UtilityField::LineStep => "line-step",
        }
    }

    /// Whether this field belongs to the surfacing wizard.
    #[must_use]
    pub const fn is_surfacing(self) -> bool {
        matches!(
            self,
            UtilityField::OriginX
                | UtilityField::OriginY
                | UtilityField::StockWidth
                | UtilityField::StockDepth
                | UtilityField::CutterDiameter
                | UtilityField::StepoverPercent
                | UtilityField::TotalDepth
                | UtilityField::DepthPerPass
                | UtilityField::Feed
                | UtilityField::PlungeFeed
                | UtilityField::SafeZ
                | UtilityField::SpindleRpm
        )
    }
}

/// Validated material surfacing parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfacingParams {
    /// Stock origin X in work coordinates, in millimetres.
    pub origin_x_mm: f64,
    /// Stock origin Y in work coordinates, in millimetres.
    pub origin_y_mm: f64,
    /// Stock extent along X, in millimetres.
    pub stock_width_mm: f64,
    /// Stock extent along Y, in millimetres.
    pub stock_depth_mm: f64,
    /// Cutter diameter, in millimetres.
    pub cutter_diameter_mm: f64,
    /// Stepover as a percentage of the cutter diameter.
    pub stepover_percent: f64,
    /// Total material to remove, in millimetres.
    pub total_depth_mm: f64,
    /// Maximum depth removed by one pass, in millimetres.
    pub depth_per_pass_mm: f64,
    /// Cutting feed, in millimetres per minute.
    pub feed_mm_min: f64,
    /// Plunge feed, in millimetres per minute.
    pub plunge_feed_mm_min: f64,
    /// Retract plane in work coordinates, in millimetres.
    pub safe_z_mm: f64,
    /// Whether the generated program starts and stops the spindle.
    pub spindle_on: bool,
    /// Spindle speed, in revolutions per minute.
    pub spindle_rpm: u32,
}

/// One generated surfacing program and its summary.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfacingPreview {
    /// Suggested program name.
    pub name: String,
    /// Complete generated G-code source.
    pub source: String,
    /// Number of depth passes.
    pub depth_passes: u32,
    /// Raster rows per depth pass.
    pub rows_per_pass: u32,
    /// Total generated source lines.
    pub line_count: usize,
    /// Total feed/plunge path length, in millimetres.
    pub path_length_mm: f64,
    /// Estimated cutting time in seconds; rapids are excluded.
    pub estimated_seconds: f64,
}

impl SurfacingPreview {
    /// Estimated cutting time as a whole number of seconds.
    #[must_use]
    pub fn estimated_secs(&self) -> u64 {
        self.estimated_seconds.ceil().max(0.0) as u64
    }
}

/// Validate and generate one bounded back-and-forth surfacing program.
///
/// The cutter center stays inside the stock outline: the first and last rows
/// and columns are inset by the cutter radius. The Z raster removes equal
/// depth slices, none deeper than `depth_per_pass_mm`.
///
/// # Errors
///
/// Returns a clear message for a non-finite or non-positive field, a stock
/// smaller than the cutter, `stepover_percent` outside `1..=100`, a spindle
/// speed of zero while the spindle is on, or a grid that would exceed the
/// line/row/pass bounds.
#[cfg(test)]
pub fn generate_surfacing(params: &SurfacingParams) -> Result<SurfacingPreview, String> {
    generate_surfacing_with_wcs(params, Wcs::G54)
}

/// Generate a surfacing program in an explicit work coordinate system.
///
/// The app passes the active WCS; the default wrapper above keeps the original
/// G54 behavior for callers and tests that do not care.
pub fn generate_surfacing_with_wcs(
    params: &SurfacingParams,
    wcs: Wcs,
) -> Result<SurfacingPreview, String> {
    validate_surfacing(params)?;

    let x_span = params.stock_width_mm - params.cutter_diameter_mm;
    let y_span = params.stock_depth_mm - params.cutter_diameter_mm;
    let row_spacing = params.cutter_diameter_mm * params.stepover_percent / 100.0;
    let rows = if y_span <= 1.0e-9 {
        1
    } else {
        ((y_span / row_spacing).ceil() as usize).saturating_add(1)
    };
    if rows > MAX_SURFACING_ROWS {
        return Err(format!(
            "raster needs {rows} rows, above the {MAX_SURFACING_ROWS}-row limit; \
             increase stepover or reduce stock depth"
        ));
    }
    let depth_passes = ((params.total_depth_mm / params.depth_per_pass_mm).ceil() as usize).max(1);
    if depth_passes > MAX_SURFACING_DEPTH_PASSES {
        return Err(format!(
            "depth needs {depth_passes} passes, above the \
             {MAX_SURFACING_DEPTH_PASSES}-pass limit; increase depth per pass"
        ));
    }

    let spindle_lines = usize::from(params.spindle_on);
    let per_pass = 2usize
        .saturating_add(rows)
        .saturating_add(rows.saturating_sub(1))
        .saturating_add(1);
    let estimated_lines = 1usize
        .saturating_add(1)
        .saturating_add(spindle_lines)
        .saturating_add(depth_passes.saturating_mul(per_pass))
        .saturating_add(1)
        .saturating_add(spindle_lines);
    if estimated_lines > MAX_SURFACING_LINES {
        return Err(format!(
            "generated program would exceed the {MAX_SURFACING_LINES}-line limit \
             ({estimated_lines} lines); increase stepover, stock size, or depth per pass"
        ));
    }

    let start_x = params.origin_x_mm + params.cutter_diameter_mm / 2.0;
    let end_x = params.origin_x_mm + params.stock_width_mm - params.cutter_diameter_mm / 2.0;
    let start_y = params.origin_y_mm + params.cutter_diameter_mm / 2.0;
    let end_y = params.origin_y_mm + params.stock_depth_mm - params.cutter_diameter_mm / 2.0;
    let actual_row_spacing = if rows > 1 {
        (end_y - start_y) / (rows.saturating_sub(1)) as f64
    } else {
        0.0
    };
    let depth_step = params.total_depth_mm / depth_passes as f64;

    let mut out = Vec::with_capacity(estimated_lines);
    let mut path_length_mm = 0.0;
    let mut plunge_length_mm = 0.0;
    out.push(format!("G21 G90 {} G17 G94", wcs.as_str()));
    out.push(format!("G0 Z{}", mm(params.safe_z_mm)));
    if params.spindle_on {
        out.push(format!("M3 S{}", params.spindle_rpm));
    }

    for pass in 0..depth_passes {
        let z = -(depth_step * (pass.saturating_add(1)) as f64);
        out.push(format!("G0 X{} Y{}", mm(start_x), mm(start_y)));
        out.push(format!(
            "G1 Z{} F{}",
            mm(z),
            rate(params.plunge_feed_mm_min)
        ));
        plunge_length_mm += (params.safe_z_mm - z).abs();
        for row in 0..rows {
            let y = if rows > 1 {
                start_y + actual_row_spacing * row as f64
            } else {
                start_y
            };
            if row > 0 {
                out.push(format!("G1 Y{} F{}", mm(y), rate(params.feed_mm_min)));
                path_length_mm += actual_row_spacing;
            }
            let x = if row % 2 == 0 { end_x } else { start_x };
            out.push(format!("G1 X{} F{}", mm(x), rate(params.feed_mm_min)));
            path_length_mm += x_span;
        }
        out.push(format!("G0 Z{}", mm(params.safe_z_mm)));
    }
    if params.spindle_on {
        out.push("M5".to_string());
    }
    out.push("M30".to_string());

    let cutting_length_mm = path_length_mm;
    let path_length_mm = cutting_length_mm + plunge_length_mm;
    let estimated_seconds = cutting_length_mm / params.feed_mm_min * 60.0
        + plunge_length_mm / params.plunge_feed_mm_min * 60.0;

    let source = out.join("\n");
    let name = format!(
        "surfacing-{}x{}-{}deep.nc",
        compact(params.stock_width_mm),
        compact(params.stock_depth_mm),
        compact(params.total_depth_mm)
    );
    Ok(SurfacingPreview {
        name,
        source,
        depth_passes: depth_passes as u32,
        rows_per_pass: rows as u32,
        line_count: out.len(),
        path_length_mm,
        estimated_seconds,
    })
}

fn validate_surfacing(params: &SurfacingParams) -> Result<(), String> {
    finite_within(
        "ORIGIN X",
        params.origin_x_mm,
        -MAX_ORIGIN_MM,
        MAX_ORIGIN_MM,
    )?;
    finite_within(
        "ORIGIN Y",
        params.origin_y_mm,
        -MAX_ORIGIN_MM,
        MAX_ORIGIN_MM,
    )?;
    positive("STOCK WIDTH", params.stock_width_mm)?;
    positive("STOCK DEPTH", params.stock_depth_mm)?;
    positive("CUTTER DIA", params.cutter_diameter_mm)?;
    if params.stock_width_mm + 1.0e-9 < params.cutter_diameter_mm {
        return Err("STOCK WIDTH must be at least the cutter diameter".to_string());
    }
    if params.stock_depth_mm + 1.0e-9 < params.cutter_diameter_mm {
        return Err("STOCK DEPTH must be at least the cutter diameter".to_string());
    }
    finite_within("STEPOVER", params.stepover_percent, 1.0, 100.0)?;
    positive("TOTAL DEPTH", params.total_depth_mm)?;
    positive("DEPTH/PASS", params.depth_per_pass_mm)?;
    positive("FEED", params.feed_mm_min)?;
    positive("PLUNGE FEED", params.plunge_feed_mm_min)?;
    positive("SAFE Z", params.safe_z_mm)?;
    if params.spindle_on && params.spindle_rpm == 0 {
        return Err("SPINDLE RPM must be greater than 0 when the spindle is on".to_string());
    }
    if params.spindle_rpm > 100_000 {
        return Err("SPINDLE RPM must be at most 100000".to_string());
    }
    Ok(())
}

fn positive(label: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    if value <= 0.0 {
        return Err(format!("{label} must be greater than 0"));
    }
    Ok(())
}

fn finite_within(label: &str, value: f64, min: f64, max: f64) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    if value < min || value > max {
        return Err(format!("{label} must be within {min:.3} ..= {max:.3}"));
    }
    Ok(())
}

fn mm(value: f64) -> String {
    format!("{:.3}", clean_zero(value))
}

fn rate(value: f64) -> String {
    format!("{:.3}", clean_zero(value))
}

fn clean_zero(value: f64) -> f64 {
    if value.abs() < 0.0005 { 0.0 } else { value }
}

fn compact(value: f64) -> String {
    if value.fract().abs() < 1.0e-9 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ------------------------------------------------------------ program limits

/// Axis-aligned bounds in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundsMm {
    /// Minimum X/Y/Z, in millimetres.
    pub min: [f64; 3],
    /// Maximum X/Y/Z, in millimetres.
    pub max: [f64; 3],
}

impl BoundsMm {
    /// Program toolpath bounds, converted from the parser's `f32` geometry.
    #[must_use]
    pub fn from_program(program: &Program) -> Option<Self> {
        if program.bounds.is_empty() {
            return None;
        }
        let min = program.bounds.min();
        let max = program.bounds.max();
        let values = [
            f64::from(min.x),
            f64::from(min.y),
            f64::from(min.z),
            f64::from(max.x),
            f64::from(max.y),
            f64::from(max.z),
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(Self {
            min: [values[0], values[1], values[2]],
            max: [values[3], values[4], values[5]],
        })
    }

    /// Shift the box by one offset per axis.
    #[must_use]
    pub fn offset(self, offset: [f64; 3]) -> Self {
        Self {
            min: [
                self.min[0] + offset[0],
                self.min[1] + offset[1],
                self.min[2] + offset[2],
            ],
            max: [
                self.max[0] + offset[0],
                self.max[1] + offset[1],
                self.max[2] + offset[2],
            ],
        }
    }

    /// Format one axis as `X -1.000 .. 10.000`.
    #[must_use]
    pub fn format_axis(&self, axis: usize) -> String {
        let name = ["X", "Y", "Z"].get(axis).copied().unwrap_or("?");
        format!(
            "{name} {} .. {}",
            mm(self.min.get(axis).copied().unwrap_or(0.0)),
            mm(self.max.get(axis).copied().unwrap_or(0.0))
        )
    }

    /// Format all three axes on one line.
    #[must_use]
    pub fn format_all(&self) -> String {
        (0..3)
            .map(|axis| self.format_axis(axis))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Soft-limit and travel values read from the controller.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TravelSettings {
    /// `$20` soft-limits enable. `None` means the controller did not report it.
    pub soft_limits: Option<bool>,
    /// `$130`/`$131`/`$132` maximum travel per X/Y/Z, in millimetres.
    pub travel_mm: [Option<f64>; 3],
}

/// One axis whose machine estimate leaves the reported travel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisOverrun {
    /// Axis label.
    pub axis: char,
    /// Controller setting number (`$130`, `$131`, or `$132`).
    pub setting: u16,
    /// Estimated machine minimum, in millimetres.
    pub min_mm: f64,
    /// Estimated machine maximum, in millimetres.
    pub max_mm: f64,
    /// Reported maximum travel, in millimetres.
    pub travel_mm: f64,
    /// The machine minimum is below zero.
    pub below_zero: bool,
    /// The machine maximum is above the reported travel.
    pub above_travel: bool,
}

impl AxisOverrun {
    /// One operator-facing line for the warning row.
    #[must_use]
    pub fn description(&self) -> String {
        match (self.below_zero, self.above_travel) {
            (true, true) => format!(
                "{} {} .. {} outside 0.000 .. ${} {}",
                self.axis,
                mm(self.min_mm),
                mm(self.max_mm),
                self.setting,
                mm(self.travel_mm)
            ),
            (true, false) => format!("{} MIN {} < 0.000", self.axis, mm(self.min_mm)),
            (false, true) => format!(
                "{} MAX {} > ${} {}",
                self.axis,
                mm(self.max_mm),
                self.setting,
                mm(self.travel_mm)
            ),
            (false, false) => String::new(),
        }
    }
}

/// Result of comparing the program against the controller's travel settings.
#[derive(Clone, Debug, PartialEq)]
pub enum LimitsStatus {
    /// Nothing is loaded.
    NoProgram,
    /// The program has no toolpath geometry.
    NoMotion,
    /// A required value was unavailable.
    Unchecked(&'static str),
    /// `$20` explicitly reports that soft limits are off.
    SoftLimitsOff,
    /// Every axis is inside the reported travel.
    WithinTravel,
    /// At least one axis leaves the reported travel.
    Exceeds(Vec<AxisOverrun>),
}

/// Read-only program limits summary for the Utility card.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramLimits {
    /// Check result.
    pub status: LimitsStatus,
    /// Program bounds in program/work coordinates, in millimetres.
    pub bounds: Option<BoundsMm>,
    /// Bounds plus the active WCS/G92 estimate, in millimetres.
    pub machine_bounds: Option<BoundsMm>,
    /// Source lines, including blank and comment-only lines.
    pub lines: usize,
    /// Toolpath segments (arc chords included).
    pub segments: usize,
    /// Blocks that commanded an arc.
    pub arcs: usize,
    /// Blocks that commanded a tool change.
    pub tool_changes: usize,
    /// Reported travel per axis, in millimetres.
    pub travel: [Option<f64>; 3],
    /// Reported `$20` value.
    pub soft_limits: Option<bool>,
}

impl ProgramLimits {
    /// Build a limits summary from the loaded program and reported settings.
    ///
    /// `work_offset_mm` is the current WCS plus `G92` offset. `None` means the
    /// controller data needed for a machine estimate was unavailable, so the
    /// travel comparison is reported as not checked instead of guessed.
    #[must_use]
    pub fn evaluate(
        program: Option<&Program>,
        travel: TravelSettings,
        work_offset_mm: Option<[f64; 3]>,
    ) -> Self {
        let Some(program) = program else {
            return Self {
                status: LimitsStatus::NoProgram,
                bounds: None,
                machine_bounds: None,
                lines: 0,
                segments: 0,
                arcs: 0,
                tool_changes: 0,
                travel: travel.travel_mm,
                soft_limits: travel.soft_limits,
            };
        };
        let lines = program.stats.lines;
        let segments = program.stats.segments;
        let arcs = program.stats.arc_moves;
        let tool_changes = program.stats.tool_changes;
        let Some(bounds) = BoundsMm::from_program(program) else {
            return Self {
                status: LimitsStatus::NoMotion,
                bounds: None,
                machine_bounds: None,
                lines,
                segments,
                arcs,
                tool_changes,
                travel: travel.travel_mm,
                soft_limits: travel.soft_limits,
            };
        };
        let mut result = Self {
            status: LimitsStatus::Unchecked("UNKNOWN"),
            bounds: Some(bounds),
            machine_bounds: None,
            lines,
            segments,
            arcs,
            tool_changes,
            travel: travel.travel_mm,
            soft_limits: travel.soft_limits,
        };

        if travel.soft_limits == Some(false) {
            result.status = LimitsStatus::SoftLimitsOff;
            return result;
        }
        let Some(soft_limits) = travel.soft_limits else {
            result.status = LimitsStatus::Unchecked("$20 NOT REPORTED");
            return result;
        };
        if !soft_limits {
            result.status = LimitsStatus::SoftLimitsOff;
            return result;
        }
        if travel.travel_mm.iter().any(Option::is_none) {
            result.status = LimitsStatus::Unchecked("$130/$131/$132 NOT REPORTED");
            return result;
        }
        let Some(work_offset) = work_offset_mm else {
            result.status = LimitsStatus::Unchecked("WCO/G92 NOT REPORTED");
            return result;
        };

        let machine = bounds.offset(work_offset);
        result.machine_bounds = Some(machine);
        let mut overruns = Vec::new();
        for (axis, label) in ["X", "Y", "Z"].iter().copied().enumerate() {
            let Some(limit) = travel.travel_mm.get(axis).copied().flatten() else {
                continue;
            };
            let min = machine.min.get(axis).copied().unwrap_or(0.0);
            let max = machine.max.get(axis).copied().unwrap_or(0.0);
            let below = min < -0.001;
            let above = max > limit + 0.001;
            if below || above {
                overruns.push(AxisOverrun {
                    axis: label.chars().next().unwrap_or('?'),
                    setting: 130 + axis as u16,
                    min_mm: min,
                    max_mm: max,
                    travel_mm: limit,
                    below_zero: below,
                    above_travel: above,
                });
            }
        }
        result.status = if overruns.is_empty() {
            LimitsStatus::WithinTravel
        } else {
            LimitsStatus::Exceeds(overruns)
        };
        result
    }

    /// Short status line for the card.
    #[must_use]
    pub fn status_text(&self) -> String {
        match &self.status {
            LimitsStatus::NoProgram => "NO PROGRAM LOADED".to_string(),
            LimitsStatus::NoMotion => "NO MOTION IN THIS PROGRAM".to_string(),
            LimitsStatus::Unchecked(reason) => format!("NOT CHECKED: {reason}"),
            LimitsStatus::SoftLimitsOff => "NOT CHECKED: SOFT LIMITS OFF ($20=0)".to_string(),
            LimitsStatus::WithinTravel => "WITHIN REPORTED TRAVEL".to_string(),
            LimitsStatus::Exceeds(overruns) => {
                let axes = overruns
                    .iter()
                    .map(|overrun| overrun.axis.to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                format!("WARNING: {axes} EXCEEDS REPORTED TRAVEL")
            }
        }
    }

    /// Whether the status is informational rather than a warning.
    #[must_use]
    pub fn is_checked(&self) -> bool {
        matches!(
            self.status,
            LimitsStatus::WithinTravel | LimitsStatus::Exceeds(_)
        )
    }

    /// Whether the status deserves the card's warning color.
    #[must_use]
    pub fn is_not_checked(&self) -> bool {
        matches!(
            self.status,
            LimitsStatus::Unchecked(_) | LimitsStatus::SoftLimitsOff
        )
    }

    /// Whether the status is an actual travel violation.
    #[must_use]
    pub fn is_over_travel(&self) -> bool {
        matches!(self.status, LimitsStatus::Exceeds(_))
    }

    /// Per-axis overrun descriptions, empty when there is no violation.
    #[must_use]
    pub fn overruns(&self) -> Vec<String> {
        match &self.status {
            LimitsStatus::Exceeds(overruns) => overruns
                .iter()
                .map(AxisOverrun::description)
                .filter(|text| !text.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Bounds text for all three axes, or a neutral placeholder.
    #[must_use]
    pub fn bounds_text(&self) -> String {
        self.bounds
            .map_or_else(|| "NO MOTION".to_string(), |bounds| bounds.format_all())
    }

    /// Travel text for all three axes, or a neutral placeholder.
    #[must_use]
    pub fn travel_text(&self) -> String {
        let mut parts = Vec::new();
        for (axis, label) in [("X", 130u16), ("Y", 131), ("Z", 132)] {
            let value = self
                .travel
                .get(label as usize - 130)
                .copied()
                .flatten()
                .map_or_else(|| "--".to_string(), mm);
            parts.push(format!("{axis} ${label} {value}"));
        }
        parts.join(" | ")
    }
}

// ------------------------------------------------------------------ transforms

/// One non-destructive source transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformKind {
    /// Prefix lines that do not already start with an `N` word.
    AddLineNumbers,
    /// Remove `(...)` blocks and `;` tails.
    StripComments,
}

impl TransformKind {
    /// Short upper-case label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            TransformKind::AddLineNumbers => "ADD LINE NUMBERS",
            TransformKind::StripComments => "STRIP COMMENTS",
        }
    }

    /// Name suffix for the transformed program.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            TransformKind::AddLineNumbers => LINE_NUMBER_SUFFIX,
            TransformKind::StripComments => STRIPPED_COMMENT_SUFFIX,
        }
    }
}

/// One transformed source and its change summary.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformPreview {
    /// Transform that produced this preview.
    pub kind: TransformKind,
    /// Name of the source program.
    pub base_name: String,
    /// Suggested name for the transformed program.
    pub name: String,
    /// Transformed source text.
    pub source: String,
    /// Source line count.
    pub input_lines: usize,
    /// Output line count.
    pub output_lines: usize,
    /// Lines whose text changed.
    pub changed_lines: usize,
    /// Line numbers added by the numbering transform.
    pub added_numbers: usize,
    /// Numbered lines left untouched.
    pub skipped_existing: usize,
    /// Lines with a `*` checksum left untouched.
    pub skipped_checksums: usize,
    /// Removed comment blocks, including `;` tails.
    pub removed_comments: usize,
    /// Height-map/original wording shown below the preview.
    pub note: String,
}

impl TransformPreview {
    /// Suggested program name for a transform.
    #[must_use]
    pub fn transformed_name(base_name: &str, kind: TransformKind) -> String {
        let base = base_name.trim();
        let base = if base.is_empty() { "program.nc" } else { base };
        format!("{base}{}", kind.suffix())
    }
}

/// Add `N` line numbers, preserving every existing line's text.
///
/// Lines already starting with an `N` word are left untouched and do not
/// advance the counter, so applying the transform twice is a no-op. Blank
/// lines and `%` markers are unchanged. A leading `/` block-delete marker
/// stays first, with the number inserted after it. Lines carrying a `*`
/// checksum are kept byte-for-byte because adding an `N` word would invalidate
/// the checksum.
///
/// # Errors
///
/// Returns a message when `start` or `step` is out of range or the counter
/// would overflow.
pub fn add_line_numbers(source: &str, start: u32, step: u32) -> Result<TransformedSource, String> {
    if step == 0 {
        return Err("line-number STEP must be at least 1".to_string());
    }
    if start > 999_999_999 {
        return Err("line-number START must be at most 999999999".to_string());
    }
    let trailing_newline = source.ends_with('\n');
    let mut output = Vec::new();
    let mut next = start;
    let mut added_numbers = 0usize;
    let mut skipped_existing = 0usize;
    let mut skipped_checksums = 0usize;
    let mut changed_lines = 0usize;
    let input_lines = split_source(source, trailing_newline).len();

    for line in split_source(source, trailing_newline) {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed.starts_with('%') {
            output.push(line.to_string());
            continue;
        }
        if has_n_word(trimmed) {
            skipped_existing += 1;
            output.push(line.to_string());
            continue;
        }
        if has_checksum(line) {
            skipped_checksums += 1;
            output.push(line.to_string());
            continue;
        }
        let indent_len = line.len() - trimmed.len();
        let (indent, body) = line.split_at(indent_len);
        let (slash, rest) = body
            .strip_prefix('/')
            .map_or((false, body), |rest| (true, rest));
        let word = format!("N{next}");
        let separator = if rest.starts_with([' ', '\t']) {
            ""
        } else {
            " "
        };
        let numbered = if slash {
            format!("{indent}/{word}{separator}{rest}")
        } else {
            format!("{indent}{word}{separator}{rest}")
        };
        output.push(numbered);
        added_numbers += 1;
        changed_lines += 1;
        next = next
            .checked_add(step)
            .ok_or_else(|| "line-number counter overflowed".to_string())?;
    }

    let mut source = output.join("\n");
    if trailing_newline {
        source.push('\n');
    }
    Ok(TransformedSource {
        source,
        input_lines,
        output_lines: output.len(),
        changed_lines,
        added_numbers,
        skipped_existing,
        skipped_checksums,
        removed_comments: 0,
    })
}

/// Remove `(...)` comment blocks and `;` tails from every line.
///
/// `%` marker lines, leading whitespace, a leading `/` block-delete marker, and
/// lines carrying a `*` checksum are preserved. Lines with no comment are
/// returned byte-for-byte, so a second pass changes nothing. Only whitespace
/// left by a removed comment is normalized; word order, numbers, and remaining
/// words are unchanged.
///
/// # Errors
///
/// Returns a message when the output would exceed the transform size bound.
pub fn strip_comments(source: &str) -> Result<TransformedSource, String> {
    let trailing_newline = source.ends_with('\n');
    let lines = split_source(source, trailing_newline);
    let mut output = Vec::with_capacity(lines.len());
    let mut changed_lines = 0usize;
    let mut removed_comments = 0usize;
    let mut skipped_checksums = 0usize;

    for line in &lines {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with('%') {
            output.push((*line).to_string());
            continue;
        }
        if has_checksum(line) {
            skipped_checksums += 1;
            output.push((*line).to_string());
            continue;
        }
        let indent_len = line.len() - trimmed.len();
        let (indent, body) = line.split_at(indent_len);
        let (slash, body) = body
            .strip_prefix('/')
            .map_or((false, body), |rest| (true, rest));
        let mut cleaned = String::with_capacity(body.len());
        let mut comment_count = 0usize;
        let mut chars = body.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                ';' => {
                    comment_count += 1;
                    break;
                }
                '(' => {
                    comment_count += 1;
                    for inner in chars.by_ref() {
                        if inner == ')' {
                            break;
                        }
                    }
                    cleaned.push(' ');
                }
                _ => cleaned.push(ch),
            }
        }
        if comment_count == 0 {
            output.push((*line).to_string());
            continue;
        }
        removed_comments += comment_count;
        changed_lines += 1;
        let words = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
        let cleaned_line = if words.is_empty() {
            if slash {
                format!("{indent}/")
            } else {
                String::new()
            }
        } else if slash {
            format!("{indent}/{words}")
        } else {
            format!("{indent}{words}")
        };
        output.push(cleaned_line);
    }

    let mut source = output.join("\n");
    if trailing_newline {
        source.push('\n');
    }
    if source.len() > MAX_TRANSFORM_BYTES {
        return Err(format!(
            "transformed source exceeds the {} MiB limit",
            MAX_TRANSFORM_BYTES / (1024 * 1024)
        ));
    }
    Ok(TransformedSource {
        source,
        input_lines: lines.len(),
        output_lines: output.len(),
        changed_lines,
        added_numbers: 0,
        skipped_existing: 0,
        skipped_checksums,
        removed_comments,
    })
}

/// Transformed source text and its change summary.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformedSource {
    /// Transformed source text.
    pub source: String,
    /// Source line count.
    pub input_lines: usize,
    /// Output line count.
    pub output_lines: usize,
    /// Lines whose text changed.
    pub changed_lines: usize,
    /// Added `N` words.
    pub added_numbers: usize,
    /// Existing `N` words left untouched.
    pub skipped_existing: usize,
    /// Checksum lines left untouched.
    pub skipped_checksums: usize,
    /// Removed comment blocks.
    pub removed_comments: usize,
}

fn split_source(source: &str, trailing_newline: bool) -> Vec<&str> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut lines = source.split('\n').collect::<Vec<_>>();
    if trailing_newline && lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn has_n_word(trimmed: &str) -> bool {
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed).trim_start();
    let mut chars = body.chars();
    matches!(chars.next(), Some('N' | 'n')) && chars.next().is_some_and(|ch| ch.is_ascii_digit())
}

fn has_checksum(line: &str) -> bool {
    line.contains('*')
}

// ---------------------------------------------------------------- utility model

/// Local utility wizard state. No preview is sent anywhere until the operator
/// confirms and the shell dispatches one explicit `LoadProgram`.
#[derive(Clone, Debug)]
pub struct UtilityModel {
    fields: [String; UtilityField::COUNT],
    field_errors: [Option<String>; UtilityField::COUNT],
    spindle_on: bool,
    surfacing_preview: Option<SurfacingPreview>,
    surfacing_error: Option<String>,
    surfacing_confirmed: bool,
    transform_preview: Option<TransformPreview>,
    transform_error: Option<String>,
    transform_confirmed: bool,
}

impl Default for UtilityModel {
    fn default() -> Self {
        Self {
            fields: std::array::from_fn(|index| {
                UtilityField::DEFAULTS
                    .get(index)
                    .map_or_else(String::new, |value| (*value).to_string())
            }),
            field_errors: std::array::from_fn(|_| None),
            spindle_on: true,
            surfacing_preview: None,
            surfacing_error: None,
            surfacing_confirmed: false,
            transform_preview: None,
            transform_error: None,
            transform_confirmed: false,
        }
    }
}

impl UtilityModel {
    /// Clear previews and confirmations for a new controller session.
    ///
    /// Field values are operator setup data and are kept across a reconnect.
    pub fn reset_session(&mut self) {
        self.surfacing_preview = None;
        self.surfacing_error = None;
        self.surfacing_confirmed = false;
        self.transform_preview = None;
        self.transform_error = None;
        self.transform_confirmed = false;
    }

    /// Field text for the editor.
    #[must_use]
    pub fn field_text(&self, field: UtilityField) -> &str {
        self.fields.get(field.index()).map_or("", String::as_str)
    }

    /// Last inline validation error for a field, if any.
    #[must_use]
    pub fn field_error(&self, field: UtilityField) -> Option<&str> {
        self.field_errors
            .get(field.index())
            .and_then(Option::as_deref)
    }

    /// Validate and store one field from the shared keypad.
    ///
    /// # Errors
    ///
    /// Returns a message for a non-numeric, non-finite, non-integer, or
    /// out-of-range value. The stored text is left unchanged on error.
    pub fn set_field_text(
        &mut self,
        field: UtilityField,
        text: impl Into<String>,
    ) -> Result<(), String> {
        let trimmed = text.into().trim().to_string();
        let result = validate_field_text(field, &trimmed);
        if let Some(slot) = self.field_errors.get_mut(field.index()) {
            *slot = result.as_ref().err().cloned();
        }
        if result.is_ok() {
            if let Some(slot) = self.fields.get_mut(field.index()) {
                *slot = trimmed;
            }
            if field.is_surfacing() {
                self.surfacing_preview = None;
                self.surfacing_error = None;
                self.surfacing_confirmed = false;
            } else {
                self.transform_preview = None;
                self.transform_error = None;
                self.transform_confirmed = false;
            }
        }
        result
    }

    /// Whether the surfacing spindle choice is on.
    #[must_use]
    pub fn spindle_on(&self) -> bool {
        self.spindle_on
    }

    /// Change the surfacing spindle choice and invalidate its preview.
    pub fn set_spindle_on(&mut self, on: bool) {
        if self.spindle_on == on {
            return;
        }
        self.spindle_on = on;
        self.surfacing_preview = None;
        self.surfacing_error = None;
        self.surfacing_confirmed = false;
    }

    /// Validate every surfacing field into a generator input.
    ///
    /// # Errors
    ///
    /// Returns the first empty, non-numeric, or out-of-range field message.
    pub fn surfacing_params(&self) -> Result<SurfacingParams, String> {
        let origin_x_mm = self.number(UtilityField::OriginX)?;
        let origin_y_mm = self.number(UtilityField::OriginY)?;
        let stock_width_mm = self.number(UtilityField::StockWidth)?;
        let stock_depth_mm = self.number(UtilityField::StockDepth)?;
        let cutter_diameter_mm = self.number(UtilityField::CutterDiameter)?;
        let stepover_percent = self.number(UtilityField::StepoverPercent)?;
        let total_depth_mm = self.number(UtilityField::TotalDepth)?;
        let depth_per_pass_mm = self.number(UtilityField::DepthPerPass)?;
        let feed_mm_min = self.number(UtilityField::Feed)?;
        let plunge_feed_mm_min = self.number(UtilityField::PlungeFeed)?;
        let safe_z_mm = self.number(UtilityField::SafeZ)?;
        let spindle_rpm = self.integer(UtilityField::SpindleRpm)?;
        Ok(SurfacingParams {
            origin_x_mm,
            origin_y_mm,
            stock_width_mm,
            stock_depth_mm,
            cutter_diameter_mm,
            stepover_percent,
            total_depth_mm,
            depth_per_pass_mm,
            feed_mm_min,
            plunge_feed_mm_min,
            safe_z_mm,
            spindle_on: self.spindle_on,
            spindle_rpm: u32::try_from(spindle_rpm).unwrap_or(0),
        })
    }

    /// Generate and cache the surfacing preview.
    ///
    /// # Errors
    ///
    /// Returns and caches the first field or limit error.
    pub fn generate_surfacing(&mut self, wcs: Wcs) -> Result<(), String> {
        self.surfacing_confirmed = false;
        let result = self
            .surfacing_params()
            .and_then(|params| generate_surfacing_with_wcs(&params, wcs));
        match result {
            Ok(preview) => {
                self.surfacing_preview = Some(preview);
                self.surfacing_error = None;
                Ok(())
            }
            Err(message) => {
                self.surfacing_preview = None;
                self.surfacing_error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Current surfacing preview, if one was generated.
    #[must_use]
    pub fn surfacing_preview(&self) -> Option<&SurfacingPreview> {
        self.surfacing_preview.as_ref()
    }

    /// Last surfacing generation error.
    #[must_use]
    pub fn surfacing_error(&self) -> Option<&str> {
        self.surfacing_error.as_deref()
    }

    /// Whether the operator acknowledged the generated-program warning.
    #[must_use]
    pub fn surfacing_confirmed(&self) -> bool {
        self.surfacing_confirmed
    }

    /// Set the surfacing acknowledgement. It can only turn on with a preview.
    pub fn set_surfacing_confirmed(&mut self, confirmed: bool) {
        self.surfacing_confirmed = confirmed && self.surfacing_preview.is_some();
    }

    /// Line-number start and step from the transform fields.
    ///
    /// # Errors
    ///
    /// Returns a message for an empty, non-integer, or out-of-range value.
    pub fn line_number_options(&self) -> Result<(u32, u32), String> {
        let start = self.integer(UtilityField::LineStart)?;
        let step = self.integer(UtilityField::LineStep)?;
        let start = u32::try_from(start).map_err(|_| "LINE START is too large".to_string())?;
        let step = u32::try_from(step).map_err(|_| "LINE STEP is too large".to_string())?;
        if step == 0 {
            return Err("LINE STEP must be at least 1".to_string());
        }
        Ok((start, step))
    }

    /// Generate and cache one transform preview from the loaded source.
    ///
    /// # Errors
    ///
    /// Returns a message when the source is empty, a numbering option is
    /// invalid, or the transformed source exceeds the size bound.
    pub fn generate_transform(
        &mut self,
        kind: TransformKind,
        base_name: &str,
        source: &str,
    ) -> Result<(), String> {
        self.transform_confirmed = false;
        let result = (|| {
            if source.trim().is_empty() {
                return Err("the loaded program source is empty".to_string());
            }
            let result = match kind {
                TransformKind::AddLineNumbers => {
                    let (start, step) = self.line_number_options()?;
                    add_line_numbers(source, start, step)?
                }
                TransformKind::StripComments => strip_comments(source)?,
            };
            if result.source.len() > MAX_TRANSFORM_BYTES {
                return Err(format!(
                    "transformed source exceeds the {} MiB limit",
                    MAX_TRANSFORM_BYTES / (1024 * 1024)
                ));
            }
            Ok(result)
        })();
        let result = match result {
            Ok(result) => result,
            Err(message) => {
                self.transform_preview = None;
                self.transform_error = Some(message.clone());
                return Err(message);
            }
        };
        let name = TransformPreview::transformed_name(base_name, kind);
        self.transform_preview = Some(TransformPreview {
            kind,
            base_name: base_name.to_string(),
            name,
            source: result.source,
            input_lines: result.input_lines,
            output_lines: result.output_lines,
            changed_lines: result.changed_lines,
            added_numbers: result.added_numbers,
            skipped_existing: result.skipped_existing,
            skipped_checksums: result.skipped_checksums,
            removed_comments: result.removed_comments,
            note: TRANSFORM_HEIGHT_MAP_NOTE.to_string(),
        });
        self.transform_error = None;
        Ok(())
    }

    /// Current transform preview, if one was generated.
    #[must_use]
    pub fn transform_preview(&self) -> Option<&TransformPreview> {
        self.transform_preview.as_ref()
    }

    /// Last transform generation error.
    #[must_use]
    pub fn transform_error(&self) -> Option<&str> {
        self.transform_error.as_deref()
    }

    /// Whether the operator acknowledged the transformed-program warning.
    #[must_use]
    pub fn transform_confirmed(&self) -> bool {
        self.transform_confirmed
    }

    /// Set the transform acknowledgement. It can only turn on with a preview.
    pub fn set_transform_confirmed(&mut self, confirmed: bool) {
        self.transform_confirmed = confirmed && self.transform_preview.is_some();
    }

    /// Discard the transform preview.
    pub fn close_transform(&mut self) {
        self.transform_preview = None;
        self.transform_confirmed = false;
        self.transform_error = None;
    }

    fn number(&self, field: UtilityField) -> Result<f64, String> {
        let text = self.field_text(field).trim();
        if text.is_empty() {
            return Err(format!("{} is empty", field.label()));
        }
        let value = text
            .parse::<f64>()
            .map_err(|_| format!("{} must be numeric", field.label()))?;
        if !value.is_finite() {
            return Err(format!("{} must be finite", field.label()));
        }
        Ok(value)
    }

    fn integer(&self, field: UtilityField) -> Result<i64, String> {
        let text = self.field_text(field).trim();
        if text.is_empty() {
            return Err(format!("{} is empty", field.label()));
        }
        text.parse::<i64>()
            .map_err(|_| format!("{} must be a whole number", field.label()))
    }
}

fn validate_field_text(field: UtilityField, text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Err(format!("{} is empty", field.label()));
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| format!("{} must be numeric", field.label()))?;
    if !value.is_finite() {
        return Err(format!("{} must be finite", field.label()));
    }
    let (min, max, integer) = match field {
        UtilityField::OriginX | UtilityField::OriginY => (-MAX_ORIGIN_MM, MAX_ORIGIN_MM, false),
        UtilityField::StockWidth | UtilityField::StockDepth => (0.001, 100_000.0, false),
        UtilityField::CutterDiameter => (0.001, 1_000.0, false),
        UtilityField::StepoverPercent => (1.0, 100.0, false),
        UtilityField::TotalDepth | UtilityField::DepthPerPass => (0.001, 10_000.0, false),
        UtilityField::Feed | UtilityField::PlungeFeed => (1.0, 100_000.0, false),
        UtilityField::SafeZ => (0.001, 100_000.0, false),
        UtilityField::SpindleRpm => (0.0, 100_000.0, true),
        UtilityField::LineStart => (0.0, 999_999_999.0, true),
        UtilityField::LineStep => (1.0, 1_000_000.0, true),
    };
    if value < min || value > max {
        return Err(format!(
            "{} must be within {} ..= {} {}",
            field.label(),
            compact(min),
            compact(max),
            field.unit()
        ));
    }
    if integer && value.fract() != 0.0 {
        return Err(format!("{} must be a whole number", field.label()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SurfacingParams {
        SurfacingParams {
            origin_x_mm: 0.0,
            origin_y_mm: 0.0,
            stock_width_mm: 20.0,
            stock_depth_mm: 10.0,
            cutter_diameter_mm: 4.0,
            stepover_percent: 50.0,
            total_depth_mm: 2.0,
            depth_per_pass_mm: 1.0,
            feed_mm_min: 900.0,
            plunge_feed_mm_min: 200.0,
            safe_z_mm: 5.0,
            spindle_on: true,
            spindle_rpm: 12_000,
        }
    }

    #[test]
    fn surfacing_uses_the_active_work_coordinate_system() {
        let preview = generate_surfacing_with_wcs(&params(), Wcs::G55).expect("valid surfacing");
        assert!(preview.source.starts_with("G21 G90 G55 G17 G94\n"));
        assert!(!preview.source.starts_with("G21 G90 G54"));
    }

    #[test]
    fn small_stock_generates_a_golden_raster_program() {
        let preview = generate_surfacing(&params()).expect("valid surfacing");
        assert_eq!(preview.depth_passes, 2);
        assert_eq!(preview.rows_per_pass, 4);
        assert_eq!(preview.name, "surfacing-20x10-2deep.nc");
        assert_eq!(
            preview.source,
            "G21 G90 G54 G17 G94\n\
             G0 Z5.000\n\
             M3 S12000\n\
             G0 X2.000 Y2.000\n\
             G1 Z-1.000 F200.000\n\
             G1 X18.000 F900.000\n\
             G1 Y4.000 F900.000\n\
             G1 X2.000 F900.000\n\
             G1 Y6.000 F900.000\n\
             G1 X18.000 F900.000\n\
             G1 Y8.000 F900.000\n\
             G1 X2.000 F900.000\n\
             G0 Z5.000\n\
             G0 X2.000 Y2.000\n\
             G1 Z-2.000 F200.000\n\
             G1 X18.000 F900.000\n\
             G1 Y4.000 F900.000\n\
             G1 X2.000 F900.000\n\
             G1 Y6.000 F900.000\n\
             G1 X18.000 F900.000\n\
             G1 Y8.000 F900.000\n\
             G1 X2.000 F900.000\n\
             G0 Z5.000\n\
             M5\n\
             M30"
        );
        assert_eq!(preview.line_count, 25);
        assert!((preview.path_length_mm - 153.0).abs() < 1.0e-9);
        assert!((preview.estimated_seconds - 13.233_333_333_333_334).abs() < 1.0e-6);
    }

    #[test]
    fn spindle_on_and_off_control_m3_and_m5() {
        let on = generate_surfacing(&params()).expect("valid");
        assert!(on.source.contains("M3 S12000"));
        assert!(on.source.contains("\nM5\n"));

        let mut off_params = params();
        off_params.spindle_on = false;
        off_params.spindle_rpm = 0;
        let off = generate_surfacing(&off_params).expect("valid without spindle");
        assert!(!off.source.contains("M3 S"));
        assert!(!off.source.contains("\nM5\n"));
        assert!(off.source.contains("M30"));
        assert_eq!(off.line_count, on.line_count - 2);
    }

    #[test]
    fn depth_passes_are_even_and_never_deeper_than_requested() {
        let mut depth_params = params();
        depth_params.total_depth_mm = 1.5;
        depth_params.depth_per_pass_mm = 1.0;
        let preview = generate_surfacing(&depth_params).expect("valid");
        assert_eq!(preview.depth_passes, 2);
        assert!(preview.source.contains("G1 Z-0.750 F200.000"));
        assert!(preview.source.contains("G1 Z-1.500 F200.000"));
        assert!(!preview.source.contains("G1 Z-1.000 F200.000"));
    }

    #[test]
    fn validation_rejects_bad_fields_with_clear_messages() {
        type Mutator = fn(&mut SurfacingParams);
        let cases: [(&str, Mutator); 10] = [
            ("stock width", |p: &mut SurfacingParams| {
                p.stock_width_mm = 0.0
            }),
            ("stock depth", |p: &mut SurfacingParams| {
                p.stock_depth_mm = f64::NAN
            }),
            ("cutter dia", |p: &mut SurfacingParams| {
                p.cutter_diameter_mm = -1.0
            }),
            ("stepover", |p: &mut SurfacingParams| {
                p.stepover_percent = 0.0
            }),
            ("total depth", |p: &mut SurfacingParams| {
                p.total_depth_mm = 0.0
            }),
            ("depth/pass", |p: &mut SurfacingParams| {
                p.depth_per_pass_mm = -2.0
            }),
            ("feed", |p: &mut SurfacingParams| p.feed_mm_min = 0.0),
            ("plunge feed", |p: &mut SurfacingParams| {
                p.plunge_feed_mm_min = f64::INFINITY
            }),
            ("safe z", |p: &mut SurfacingParams| p.safe_z_mm = 0.0),
            ("spindle rpm", |p: &mut SurfacingParams| p.spindle_rpm = 0),
        ];
        for (label, mutate) in cases {
            let mut bad = params();
            mutate(&mut bad);
            let error = generate_surfacing(&bad).expect_err(label);
            assert!(
                error.to_ascii_lowercase().contains(label),
                "{label}: {error}"
            );
        }
    }

    #[test]
    fn stock_must_fit_the_cutter() {
        let mut bad = params();
        bad.cutter_diameter_mm = 30.0;
        let error = generate_surfacing(&bad).expect_err("stock is too small");
        assert!(error.contains("at least the cutter diameter"), "{error}");
    }

    #[test]
    fn raster_line_cap_rejects_a_large_grid() {
        let mut big = params();
        big.stock_width_mm = 5_000.0;
        big.stock_depth_mm = 5_000.0;
        big.cutter_diameter_mm = 1.0;
        big.stepover_percent = 1.0;
        let error = generate_surfacing(&big).expect_err("grid is capped");
        assert!(
            error.contains("limit") || error.contains("exceed"),
            "{error}"
        );
    }

    #[test]
    fn field_validation_keeps_the_text_on_error() {
        let mut model = UtilityModel::default();
        assert_eq!(model.field_text(UtilityField::StockWidth), "100");
        assert!(model.set_field_text(UtilityField::StockWidth, "0").is_err());
        assert_eq!(model.field_text(UtilityField::StockWidth), "100");
        assert!(model.field_error(UtilityField::StockWidth).is_some());
        model
            .set_field_text(UtilityField::StockWidth, "120")
            .expect("valid width");
        assert!(model.field_error(UtilityField::StockWidth).is_none());
    }

    fn limits_program() -> Program {
        ghs_gcode::parse_program(
            "G21\nG0 X10 Y20 Z-1\nG1 X100 F100\nM6 T2\nG2 X100 Y0 I0 J-10\nM30\n",
        )
        .expect("limits program parses")
    }

    #[test]
    fn limits_report_bounds_counts_and_within_travel() {
        let limits = ProgramLimits::evaluate(
            Some(&limits_program()),
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(200.0), Some(200.0), Some(100.0)],
            },
            Some([0.0, 0.0, 1.0]),
        );
        assert!(
            matches!(limits.status, LimitsStatus::WithinTravel),
            "{:?}",
            limits.status
        );
        assert_eq!(limits.status_text(), "WITHIN REPORTED TRAVEL");
        assert_eq!(limits.lines, 6);
        assert!(limits.segments > 0);
        assert_eq!(limits.arcs, 1);
        assert_eq!(limits.tool_changes, 1);
        assert_eq!(
            limits.bounds_text(),
            "X 0.000 .. 110.000 | Y 0.000 .. 20.000 | Z -1.000 .. 0.000"
        );
        assert_eq!(
            limits.travel_text(),
            "X $130 200.000 | Y $131 200.000 | Z $132 100.000"
        );
    }

    #[test]
    fn limits_with_work_offset_warn_when_machine_bounds_exceed_travel() {
        let limits = ProgramLimits::evaluate(
            Some(&limits_program()),
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(100.0), Some(200.0), Some(100.0)],
            },
            Some([10.0, 0.0, 10.0]),
        );
        assert!(limits.is_over_travel());
        assert_eq!(limits.status_text(), "WARNING: X EXCEEDS REPORTED TRAVEL");
        assert_eq!(
            limits.overruns(),
            vec!["X MAX 120.000 > $130 100.000".to_string()]
        );
    }

    #[test]
    fn limits_not_checked_when_soft_limits_are_off_or_unknown() {
        let program = limits_program();
        let off = ProgramLimits::evaluate(
            Some(&program),
            TravelSettings {
                soft_limits: Some(false),
                travel_mm: [Some(100.0); 3],
            },
            Some([0.0; 3]),
        );
        assert!(off.is_not_checked());
        assert_eq!(off.status_text(), "NOT CHECKED: SOFT LIMITS OFF ($20=0)");

        let unknown = ProgramLimits::evaluate(
            Some(&program),
            TravelSettings {
                soft_limits: None,
                travel_mm: [Some(100.0); 3],
            },
            Some([0.0; 3]),
        );
        assert_eq!(unknown.status_text(), "NOT CHECKED: $20 NOT REPORTED");
    }

    #[test]
    fn limits_not_checked_when_travel_or_work_offset_is_missing() {
        let program = limits_program();
        let missing_travel = ProgramLimits::evaluate(
            Some(&program),
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(100.0), None, Some(100.0)],
            },
            Some([0.0; 3]),
        );
        assert_eq!(
            missing_travel.status_text(),
            "NOT CHECKED: $130/$131/$132 NOT REPORTED"
        );

        let missing_wco = ProgramLimits::evaluate(
            Some(&program),
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(100.0); 3],
            },
            None,
        );
        assert_eq!(
            missing_wco.status_text(),
            "NOT CHECKED: WCO/G92 NOT REPORTED"
        );
    }

    #[test]
    fn limits_report_no_program_and_no_motion() {
        let none = ProgramLimits::evaluate(
            None,
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(100.0); 3],
            },
            Some([0.0; 3]),
        );
        assert_eq!(none.status_text(), "NO PROGRAM LOADED");

        let comments = ghs_gcode::parse_program("(only a comment)\n").expect("parse");
        let no_motion = ProgramLimits::evaluate(
            Some(&comments),
            TravelSettings {
                soft_limits: Some(true),
                travel_mm: [Some(100.0); 3],
            },
            Some([0.0; 3]),
        );
        assert_eq!(no_motion.status_text(), "NO MOTION IN THIS PROGRAM");
    }

    #[test]
    fn line_numbers_are_golden_and_skip_existing_words() {
        let source = "%\nG21\nN100 G0 X0\n(comment)\n/G0 Z5\nG1 X10 *12\n";
        let result = add_line_numbers(source, 10, 10).expect("numbering");
        assert_eq!(
            result.source,
            "%\nN10 G21\nN100 G0 X0\nN20 (comment)\n/N30 G0 Z5\nG1 X10 *12\n"
        );
        assert_eq!(result.added_numbers, 3);
        assert_eq!(result.skipped_existing, 1);
        assert_eq!(result.skipped_checksums, 1);
        assert_eq!(result.input_lines, 6);
        assert_eq!(result.output_lines, 6);
    }

    #[test]
    fn line_numbers_are_idempotent_and_handle_blank_lines() {
        let source = "\nG21\n\nG0 X0\n";
        let once = add_line_numbers(source, 10, 10).expect("numbering");
        assert_eq!(once.source, "\nN10 G21\n\nN20 G0 X0\n");
        let twice = add_line_numbers(&once.source, 10, 10).expect("numbering again");
        assert_eq!(twice.source, once.source);
        assert_eq!(twice.added_numbers, 0);
        assert_eq!(twice.skipped_existing, 2);
    }

    #[test]
    fn line_numbers_reject_bad_options_and_prevent_overflow() {
        assert!(add_line_numbers("G21\n", 10, 0).is_err());
        assert!(add_line_numbers("G21\n", 1_000_000_000, 10).is_err());
        let error = add_line_numbers("G21\nG0 X0\n", 0, u32::MAX).expect_err("overflow");
        assert!(error.contains("overflow"), "{error}");
    }

    #[test]
    fn strip_comments_is_golden_and_preserves_markers() {
        let source = "%\n/ G0 X0 Y0 (park)\nG1 X10 ; advance\n(header)\nG1 (mid) X20\nM3 S12000 (start) ; tail\n";
        let result = strip_comments(source).expect("strip");
        assert_eq!(result.source, "%\n/G0 X0 Y0\nG1 X10\n\nG1 X20\nM3 S12000\n");
        assert_eq!(result.removed_comments, 6);
        assert_eq!(result.changed_lines, 5);
        assert_eq!(result.input_lines, 6);
        assert_eq!(result.output_lines, 6);
    }

    #[test]
    fn strip_comments_is_idempotent_and_handles_edge_cases() {
        let source = "G1 X1 (open\nG1 X2 ; tail\nG1 X3 (a;b) Y4\n";
        let once = strip_comments(source).expect("strip");
        assert_eq!(once.source, "G1 X1\nG1 X2\nG1 X3 Y4\n");
        let twice = strip_comments(&once.source).expect("strip again");
        assert_eq!(twice.source, once.source);
        assert_eq!(twice.removed_comments, 0);
        assert_eq!(twice.changed_lines, 0);
    }

    #[test]
    fn strip_comments_keeps_checksum_lines_byte_for_byte() {
        let source = "G1 X10 (cut) *58\nG1 X20 ; next\n";
        let result = strip_comments(source).expect("strip");
        assert_eq!(result.source, "G1 X10 (cut) *58\nG1 X20\n");
        assert_eq!(result.skipped_checksums, 1);
        assert_eq!(result.removed_comments, 1);
    }

    #[test]
    fn transform_preview_names_and_height_map_note_are_clear() {
        assert_eq!(
            TransformPreview::transformed_name("part.nc", TransformKind::AddLineNumbers),
            "part.nc [line numbers]"
        );
        assert_eq!(
            TransformPreview::transformed_name("part.nc", TransformKind::StripComments),
            "part.nc [comments stripped]"
        );
        assert!(TRANSFORM_HEIGHT_MAP_NOTE.contains("height-map Apply/Revert"));
        assert!(TRANSFORM_HEIGHT_MAP_NOTE.contains("transformed source"));
    }

    #[test]
    fn utility_model_generates_previews_and_requires_confirmation() {
        let mut model = UtilityModel::default();
        model.generate_surfacing(Wcs::G54).expect("preview");
        assert!(model.surfacing_preview().is_some());
        assert!(!model.surfacing_confirmed());
        model.set_surfacing_confirmed(true);
        assert!(model.surfacing_confirmed());
        model
            .set_field_text(UtilityField::StockWidth, "120")
            .expect("valid edit");
        assert!(model.surfacing_preview().is_none());
        assert!(!model.surfacing_confirmed());

        model
            .generate_transform(TransformKind::AddLineNumbers, "part.nc", "G21\nG0 X0\n")
            .expect("transform");
        let preview = model.transform_preview().expect("transform preview");
        assert_eq!(preview.name, "part.nc [line numbers]");
        assert_eq!(preview.source, "N10 G21\nN20 G0 X0\n");
        assert!(!model.transform_confirmed());
        model.set_transform_confirmed(true);
        assert!(model.transform_confirmed());
        model.close_transform();
        assert!(model.transform_preview().is_none());
        assert!(!model.transform_confirmed());
    }
}
