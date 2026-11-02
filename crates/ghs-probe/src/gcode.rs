//! G-code plan construction and the safety preamble.

use ghs_core::{Axis, Position, Wcs};

use crate::error::ProbeError;
use crate::params::{ProbeParams, check_coordinate, check_finite, check_position};
use crate::result::CycleKind;

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
    use super::*;

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
}
