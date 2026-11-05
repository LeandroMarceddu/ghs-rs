//! G-code plan construction and the safety preamble.

use ghs_core::{Axis, Position, Wcs};

use crate::error::{ProbeError, ProbeMacroError};
use crate::macros::validate_probe_macro;
use crate::params::{ProbeParams, check_coordinate, check_finite, check_position};
use crate::result::CycleKind;

/// Inclusive line budget for a probe program after operator macros are added.
///
/// This mirrors the 10,000-line class bound used for generated programs
/// elsewhere in the workspace; a plan that would cross it is refused so the
/// preflight sheet and the streamer stay bounded.
const MAX_PROBE_PROGRAM_LINES: usize = 10_000;

/// Number of lines emitted by [`GcodeBuilder::new`] before any cycle move.
const PREAMBLE_LINES: usize = 5;

/// How a `G38` probe move terminates.
///
/// All cycle plans use [`ProbeMoveCode::G38_2`]. The other codes are modelled
/// so the engine can render or compare plans without string matching, but no
/// builder emits them: `G38.3`/`G38.5` continue after a miss and `G38.4`
/// searches away from the surface, and a missed probe must stop the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeMoveCode {
    /// `G38.2`: probe towards the target; alarm on no contact.
    G38_2,
    /// `G38.3`: probe towards the target; report position on no contact.
    G38_3,
    /// `G38.4`: probe away from the target; alarm on no contact break.
    G38_4,
    /// `G38.5`: probe away from the target; report position on no contact
    /// break.
    G38_5,
}

impl ProbeMoveCode {
    /// Canonical G-code word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProbeMoveCode::G38_2 => "G38.2",
            ProbeMoveCode::G38_3 => "G38.3",
            ProbeMoveCode::G38_4 => "G38.4",
            ProbeMoveCode::G38_5 => "G38.5",
        }
    }

    /// Whether a missed search raises a controller alarm.
    #[must_use]
    pub const fn alarms_on_failure(self) -> bool {
        matches!(self, ProbeMoveCode::G38_2 | ProbeMoveCode::G38_4)
    }

    /// Whether the move stops when the probe makes contact.
    #[must_use]
    pub const fn stops_on_contact(self) -> bool {
        matches!(self, ProbeMoveCode::G38_2 | ProbeMoveCode::G38_3)
    }
}

/// A unit direction vector for one probe move.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProbeVector {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

impl ProbeVector {
    /// Build a vector.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        ProbeVector { x, y, z }
    }

    /// Euclidean length.
    #[must_use]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y).hypot(self.z)
    }

    /// Unit vector, or `None` for a zero or non-finite vector.
    #[must_use]
    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        let unit = ProbeVector::new(self.x / length, self.y / length, self.z / length);
        unit.is_finite().then_some(unit)
    }

    /// Returns `true` when all components are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

/// One `G38` move in a plan.
///
/// The app correlates the Nth engine `Probe` event with the probe at the same
/// index. Labels are stable and human-readable for the preflight sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedProbe {
    /// Zero-based index in the plan.
    pub index: usize,
    /// Stable label, for example `"x-edge"` or `"touch-1"`.
    pub label: String,
    /// Probe move code. Always [`ProbeMoveCode::G38_2`] today.
    pub code: ProbeMoveCode,
    /// Controlled-point machine position at the start of the search.
    ///
    /// This is the commanded position, matching the emitted program after the
    /// WCS offset. When a probe-tip offset is configured, the physical tip is
    /// at `from + offset`; the offset is not applied to this field.
    pub from: Position,
    /// Absolute controlled-point machine target of the search.
    ///
    /// As with [`PlannedProbe::from`], add the probe-tip offset to obtain the
    /// feature/tip coordinate.
    pub to: Position,
    /// Unit direction from `from` to `to`.
    pub direction: ProbeVector,
    /// Search travel, in millimetres.
    pub travel_mm: f64,
    /// Feed, in millimetres per minute.
    pub feed_mm_min: f64,
}

/// Conditions the app must verify before it starts a probe plan.
///
/// Every `requires_*` flag is `true`: the plan is only legal when the condition
/// holds. The app owns the actual check and the operator confirmation; the
/// engine safety gateway remains the final authority.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbePreconditions {
    /// A probe input or verified probe source is required.
    pub requires_probe: bool,
    /// Probe connection or an explicit operator confirmation is required.
    pub requires_probe_verification: bool,
    /// The spindle must be stopped.
    pub requires_spindle_off: bool,
    /// Coolant must be off.
    pub requires_coolant_off: bool,
    /// The machine must be idle and the link ready.
    pub requires_idle: bool,
    /// The operator must confirm the direction and travel.
    pub requires_operator_confirmation: bool,
    /// Safe Z plane, in machine millimetres.
    pub safe_z_mm: f64,
    /// Maximum search travel, in millimetres.
    pub search_distance_mm: f64,
}

impl ProbePreconditions {
    /// Build the preconditions implied by shared parameters.
    #[must_use]
    pub fn from_params(params: &ProbeParams) -> Self {
        ProbePreconditions {
            requires_probe: true,
            requires_probe_verification: true,
            requires_spindle_off: true,
            requires_coolant_off: true,
            requires_idle: true,
            requires_operator_confirmation: true,
            safe_z_mm: params.safe_z_mm,
            search_distance_mm: params.search_distance_mm,
        }
    }
}

/// A complete, bounded probe program.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbePlan {
    /// Stable plan name, for example `"edge-x-outside-positive"`.
    pub name: String,
    /// Cycle kind, including `HeightMap`.
    pub kind: CycleKind,
    /// Complete G-code program in [`ProbePlan::wcs`], newline separated with
    /// no trailing newline.
    ///
    /// All XY coordinates in the program are **controlled-point** positions;
    /// a probe-tip offset was already subtracted from the feature geometry.
    pub program: String,
    /// Every probe move, in execution order.
    ///
    /// Its `from`/`to` positions are **controlled-point** machine coordinates
    /// (the commanded position), not feature/tip coordinates. Add the
    /// configured probe-tip offset to obtain the feature position. The emitted
    /// `program` is in work coordinates.
    pub probes: Vec<PlannedProbe>,
    /// Number of `PRB` results the cycle expects; always `probes.len()`.
    pub expected_probes: usize,
    /// Conditions the app must verify before starting.
    pub preconditions: ProbePreconditions,
    /// Work coordinate system selected by the program.
    pub wcs: Wcs,
    /// Safe Z plane, in machine millimetres.
    pub safe_z_mm: f64,
}

impl ProbePlan {
    /// Sum of the search travels of every probe, in millimetres.
    #[must_use]
    pub fn total_search_travel_mm(&self) -> f64 {
        self.probes.iter().map(|probe| probe.travel_mm).sum()
    }

    /// Program lines, without the trailing newline.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.program.lines()
    }

    /// Return this plan with bounded operator macros around the probe moves.
    ///
    /// `pre` is inserted immediately after the fixed safety preamble
    /// (`G21 G90 G17 G94`, the WCS, `M5`, `M9`, and the initial safe-Z rapid)
    /// and before the first XY or probe move. `post` is appended after the
    /// final retract and safe-Z rapid.
    ///
    /// Both macros are validated with [`validate_probe_macro`], so they can add
    /// comments, a `G4 P<seconds>` dwell in `0..=60`, and `M0`/`M1`/`M5`/`M9`
    /// only. They never add motion, probing, spindle or coolant start, a tool
    /// change, a coordinate change, or a system command.
    ///
    /// Empty or blank-only macros return the plan unchanged. `probes`,
    /// `expected_probes`, `preconditions`, `wcs`, and `safe_z_mm` are never
    /// modified.
    ///
    /// # Errors
    ///
    /// Returns [`ProbeMacroError`] when a macro is invalid, when the program
    /// does not start with the fixed safety preamble, or when the result would
    /// exceed a 10,000-line program budget. The plan is left untouched on
    /// error.
    pub fn with_macros(self, pre: &str, post: &str) -> Result<Self, ProbeMacroError> {
        let pre_lines = validate_probe_macro(pre)?;
        let post_lines = validate_probe_macro(post)?;
        self.with_clean_macros(&pre_lines, &post_lines)
    }

    fn with_clean_macros(
        mut self,
        pre: &[String],
        post: &[String],
    ) -> Result<Self, ProbeMacroError> {
        if pre.is_empty() && post.is_empty() {
            return Ok(self);
        }
        let lines: Vec<&str> = self.program.split('\n').collect();
        let insert_at = preamble_end(&lines)?;
        if !lines.last().is_some_and(|line| line.starts_with("G0 Z")) {
            return Err(ProbeMacroError::ProgramShape);
        }
        let total = lines.len() + pre.len() + post.len();
        if total > MAX_PROBE_PROGRAM_LINES {
            return Err(ProbeMacroError::ProgramTooLong {
                lines: total,
                max: MAX_PROBE_PROGRAM_LINES,
            });
        }

        let mut program: Vec<String> = Vec::with_capacity(total);
        program.extend(lines[..insert_at].iter().map(|line| (*line).to_string()));
        program.extend(pre.iter().cloned());
        program.extend(lines[insert_at..].iter().map(|line| (*line).to_string()));
        program.extend(post.iter().cloned());
        self.program = program.join("\n");
        Ok(self)
    }
}

/// End of the fixed safety preamble, before any XY or probe move.
fn preamble_end(lines: &[&str]) -> Result<usize, ProbeMacroError> {
    let matches = lines.len() >= PREAMBLE_LINES
        && lines[0] == "G21 G90 G17 G94"
        && is_wcs_line(lines[1])
        && lines[2].eq_ignore_ascii_case("M5")
        && lines[3].eq_ignore_ascii_case("M9")
        && lines[4].starts_with("G0 Z");
    matches
        .then_some(PREAMBLE_LINES)
        .ok_or(ProbeMacroError::ProgramShape)
}

fn is_wcs_line(line: &str) -> bool {
    matches!(
        line,
        "G54" | "G55" | "G56" | "G57" | "G58" | "G59" | "G59.1" | "G59.2" | "G59.3"
    )
}

/// Builder shared by all cycles.
pub(crate) struct GcodeBuilder<'a> {
    params: &'a ProbeParams,
    lines: Vec<String>,
    probes: Vec<PlannedProbe>,
    current: [Option<f64>; 3],
}

impl<'a> GcodeBuilder<'a> {
    /// Start a program with the fixed safety preamble.
    pub(crate) fn new(params: &'a ProbeParams) -> Self {
        let mut builder = GcodeBuilder {
            params,
            lines: Vec::new(),
            probes: Vec::new(),
            current: [None; 3],
        };
        builder.lines.push("G21 G90 G17 G94".to_string());
        builder.lines.push(params.wcs.as_str().to_string());
        builder.lines.push("M5".to_string());
        builder.lines.push("M9".to_string());
        builder.rapid_z_unchecked(params.safe_z_mm);
        builder
    }

    /// Rapid in XY, preserving Z.
    ///
    /// `x_mm` and `y_mm` are machine coordinates; the emitted line is in the
    /// plan's work coordinate system.
    pub(crate) fn rapid_xy(&mut self, x_mm: f64, y_mm: f64) -> Result<(), ProbeError> {
        check_coordinate("rapid X", x_mm)?;
        check_coordinate("rapid Y", y_mm)?;
        let work_x = x_mm - self.params.wcs_offset_mm.get(Axis::X);
        let work_y = y_mm - self.params.wcs_offset_mm.get(Axis::Y);
        check_finite("work rapid X", work_x)?;
        check_finite("work rapid Y", work_y)?;
        self.lines
            .push(format!("G0 X{} Y{}", format_mm(work_x), format_mm(work_y)));
        self.set_current(Axis::X, work_x);
        self.set_current(Axis::Y, work_y);
        Ok(())
    }

    /// Rapid in Z, preserving XY.
    ///
    /// `z_mm` is a machine coordinate; the emitted line is in the plan's work
    /// coordinate system.
    pub(crate) fn rapid_z(&mut self, z_mm: f64) -> Result<(), ProbeError> {
        check_coordinate("rapid Z", z_mm)?;
        self.rapid_z_unchecked(z_mm);
        Ok(())
    }

    fn rapid_z_unchecked(&mut self, z_mm: f64) {
        let work_z = z_mm - self.params.wcs_offset_mm.get(Axis::Z);
        self.lines.push(format!("G0 Z{}", format_mm(work_z)));
        self.set_current(Axis::Z, work_z);
    }

    /// Emit one `G38.2` move and record it.
    pub(crate) fn probe(
        &mut self,
        label: impl Into<String>,
        from: Position,
        to: Position,
    ) -> Result<(), ProbeError> {
        check_position("probe start", from)?;
        check_position("probe target", to)?;
        let from_work = self.params.to_work(from);
        let to_work = self.params.to_work(to);
        let delta = ProbeVector::new(
            to.get(Axis::X) - from.get(Axis::X),
            to.get(Axis::Y) - from.get(Axis::Y),
            to.get(Axis::Z) - from.get(Axis::Z),
        );
        let direction = delta
            .normalized()
            .ok_or(ProbeError::Degenerate("probe move has zero length"))?;

        let mut words = Vec::new();
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let distance = (to_work.get(axis) - from_work.get(axis)).abs();
            if distance <= 0.000_001 {
                continue;
            }
            let current = self
                .current
                .get(axis.index())
                .copied()
                .flatten()
                .ok_or(ProbeError::UnestablishedPosition(axis.as_str()))?;
            if (current - from_work.get(axis)).abs() > 0.000_001 {
                return Err(ProbeError::UnestablishedPosition(axis.as_str()));
            }
            words.push(format!("{}{}", axis.letter(), format_mm(to_work.get(axis))));
        }
        if words.is_empty() {
            return Err(ProbeError::Degenerate("probe move has no axis words"));
        }
        self.lines.push(format!(
            "G38.2 {} F{}",
            words.join(" "),
            format_mm(self.params.feed_mm_min)
        ));
        self.probes.push(PlannedProbe {
            index: self.probes.len(),
            label: label.into(),
            code: ProbeMoveCode::G38_2,
            from,
            to,
            direction,
            travel_mm: delta.length(),
            feed_mm_min: self.params.feed_mm_min,
        });
        self.current = [None; 3];
        Ok(())
    }

    /// Lift by the retract distance and then move to safe Z.
    pub(crate) fn retract(&mut self) -> Result<(), ProbeError> {
        check_finite("retract_mm", self.params.retract_mm)?;
        check_finite("safe_z_mm", self.params.safe_z_mm)?;
        self.lines.push("G91".to_string());
        self.lines
            .push(format!("G0 Z{}", format_mm(self.params.retract_mm)));
        self.lines.push("G90".to_string());
        self.rapid_z_unchecked(self.params.safe_z_mm);
        self.current[Axis::X.index()] = None;
        self.current[Axis::Y.index()] = None;
        Ok(())
    }

    /// Finish the program.
    pub(crate) fn finish(
        self,
        kind: CycleKind,
        name: impl Into<String>,
    ) -> Result<ProbePlan, ProbeError> {
        if self.probes.is_empty() {
            return Err(ProbeError::EmptyPlan);
        }
        let expected_probes = self.probes.len();
        Ok(ProbePlan {
            name: name.into(),
            kind,
            program: self.lines.join("\n"),
            probes: self.probes,
            expected_probes,
            preconditions: ProbePreconditions::from_params(self.params),
            wcs: self.params.wcs,
            safe_z_mm: self.params.safe_z_mm,
        })
    }

    fn set_current(&mut self, axis: Axis, value: f64) {
        self.current[axis.index()] = Some(value);
    }
}

/// Format a millimetre value with three decimals, normalising negative zero.
pub(crate) fn format_mm(value: f64) -> String {
    let rounded = (value * 1_000.0).round() / 1_000.0;
    if rounded == 0.0 || rounded.abs() < 0.000_5 {
        "0.000".to_string()
    } else {
        format!("{rounded:.3}")
    }
}

#[cfg(test)]
mod tests {
    use ghs_core::{Position, Wcs};

    use super::*;
    use crate::cycle::{EdgeSpec, ProbeCycle};
    use crate::params::{EdgeMode, ProbeAxis, Sign};

    #[test]
    fn probe_codes_expose_their_semantics() {
        assert_eq!(ProbeMoveCode::G38_2.as_str(), "G38.2");
        assert!(ProbeMoveCode::G38_2.alarms_on_failure());
        assert!(ProbeMoveCode::G38_2.stops_on_contact());
        assert!(!ProbeMoveCode::G38_3.alarms_on_failure());
        assert!(ProbeMoveCode::G38_3.stops_on_contact());
        assert!(ProbeMoveCode::G38_4.alarms_on_failure());
        assert!(!ProbeMoveCode::G38_4.stops_on_contact());
        assert!(!ProbeMoveCode::G38_5.alarms_on_failure());
        assert!(!ProbeMoveCode::G38_5.stops_on_contact());
    }

    #[test]
    fn number_formatting_is_fixed_and_normalises_negative_zero() {
        assert_eq!(format_mm(1.234_56), "1.235");
        assert_eq!(format_mm(-0.0), "0.000");
        assert_eq!(format_mm(-0.000_1), "0.000");
        assert_eq!(format_mm(-12.5), "-12.500");
    }

    fn sample_plan() -> ProbePlan {
        let params = ProbeParams {
            safe_z_mm: 10.0,
            search_distance_mm: 5.0,
            feed_mm_min: 100.0,
            retract_mm: 2.0,
            probe_diameter_mm: 4.0,
            position: Some(Position::xyz(0.0, 0.0, -2.0)),
            wcs: Wcs::G54,
            ..ProbeParams::default()
        };
        let cycle = ProbeCycle::Edge(EdgeSpec {
            axis: ProbeAxis::X,
            mode: EdgeMode::Outside,
            approach: Sign::Positive,
            expected_edge_mm: 20.0,
        });
        cycle.plan(&params).expect("sample plan")
    }

    /// A plan with exactly `total_lines` program lines and a valid preamble.
    fn padded_plan(total_lines: usize) -> ProbePlan {
        let mut plan = sample_plan();
        let mut lines = vec![
            "G21 G90 G17 G94".to_string(),
            "G54".to_string(),
            "M5".to_string(),
            "M9".to_string(),
            "G0 Z10.000".to_string(),
        ];
        while lines.len() + 1 < total_lines {
            lines.push("(pad)".to_string());
        }
        lines.push("G0 Z10.000".to_string());
        plan.program = lines.join("\n");
        plan
    }

    #[test]
    fn empty_macros_leave_the_plan_unchanged() {
        let plan = sample_plan();
        assert_eq!(plan.clone().with_macros("", "").expect("empty"), plan);
        assert_eq!(
            plan.clone().with_macros(" \n\t\n", "").expect("blank"),
            plan
        );
        assert_eq!(plan.clone().with_macros("", "\n").expect("blank"), plan);
    }

    #[test]
    fn pre_macro_follows_the_safe_z_rapid_and_precedes_the_first_move() {
        let plan = sample_plan()
            .with_macros("(pre)\nG4 P1", "")
            .expect("macros");
        let lines: Vec<&str> = plan.lines().collect();
        assert_eq!(
            lines[..7],
            [
                "G21 G90 G17 G94",
                "G54",
                "M5",
                "M9",
                "G0 Z10.000",
                "(pre)",
                "G4 P1",
            ]
        );
        let first_xy = lines
            .iter()
            .position(|line| line.starts_with("G0 X"))
            .expect("XY rapid");
        let first_probe = lines
            .iter()
            .position(|line| line.starts_with("G38.2"))
            .expect("probe");
        assert_eq!(first_xy, 7);
        assert!(first_probe > first_xy);
    }

    #[test]
    fn post_macro_follows_the_final_retract_and_safe_z() {
        let plan = sample_plan().with_macros("", "; done\nM0").expect("macros");
        let lines: Vec<&str> = plan.lines().collect();
        let count = lines.len();
        assert_eq!(&lines[count - 2..], ["; done", "M0"]);
        assert_eq!(lines[count - 3], "G0 Z10.000");
        assert_eq!(lines[count - 4], "G90");
        let last_probe = lines
            .iter()
            .rposition(|line| line.starts_with("G38.2"))
            .expect("probe");
        assert!(last_probe < count - 3);
    }

    #[test]
    fn with_macros_preserves_plan_metadata() {
        let base = sample_plan();
        let plan = base.clone().with_macros("(pre)", "M5").expect("macros");
        assert_eq!(plan.name, base.name);
        assert_eq!(plan.kind, base.kind);
        assert_eq!(plan.probes, base.probes);
        assert_eq!(plan.expected_probes, base.expected_probes);
        assert_eq!(plan.expected_probes, 1);
        assert_eq!(plan.preconditions, base.preconditions);
        assert_eq!(plan.wcs, base.wcs);
        assert_eq!(plan.safe_z_mm, base.safe_z_mm);
    }

    #[test]
    fn with_macros_only_inserts_the_validated_lines() {
        let base = sample_plan();
        let mut expected: Vec<String> = base.lines().map(str::to_string).collect();
        expected.splice(
            5..5,
            ["(pre)", "G4 P2", "M0", "M1", "M5", "M9"].map(str::to_string),
        );
        expected.extend(["; done", "M5"].map(str::to_string));

        let plan = base
            .with_macros("(pre)\nG4 P2\nM0\nM1\nM5\nM9", "; done\nM5")
            .expect("macros");
        assert!(plan.lines().eq(expected.iter().map(String::as_str)));
    }

    #[test]
    fn with_macros_rejects_a_forbidden_line() {
        let base = sample_plan();
        let error = base
            .clone()
            .with_macros("G0 X0", "")
            .expect_err("motion must be rejected");
        assert!(matches!(
            error,
            ProbeMacroError::ForbiddenLine { line: 1, .. }
        ));

        let error = base
            .clone()
            .with_macros("", "M3 S1000")
            .expect_err("spindle start must be rejected");
        assert!(matches!(error, ProbeMacroError::ForbiddenLine { .. }));
        assert_eq!(base.program, sample_plan().program);
    }

    #[test]
    fn with_macros_requires_the_fixed_safety_preamble() {
        let mut plan = sample_plan();
        plan.program = "G21 G90\nG38.2 Z0".to_string();
        assert_eq!(
            plan.with_macros("(pre)", "").expect_err("shape"),
            ProbeMacroError::ProgramShape
        );

        let mut empty = sample_plan();
        empty.program = String::new();
        assert!(empty.clone().with_macros("", "").is_ok());
        assert_eq!(
            empty.with_macros("(pre)", "").expect_err("shape"),
            ProbeMacroError::ProgramShape
        );
    }

    #[test]
    fn with_macros_enforces_the_program_line_budget() {
        let at_limit = padded_plan(MAX_PROBE_PROGRAM_LINES - 1)
            .with_macros("(pre)", "")
            .expect("one macro line fits");
        assert_eq!(at_limit.lines().count(), MAX_PROBE_PROGRAM_LINES);

        let error = padded_plan(MAX_PROBE_PROGRAM_LINES)
            .with_macros("(pre)", "")
            .expect_err("over budget");
        assert_eq!(
            error,
            ProbeMacroError::ProgramTooLong {
                lines: MAX_PROBE_PROGRAM_LINES + 1,
                max: MAX_PROBE_PROGRAM_LINES,
            }
        );
    }
}
