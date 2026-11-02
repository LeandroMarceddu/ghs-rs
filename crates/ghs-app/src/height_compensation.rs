//! Explicit **Apply height map** / **Revert** workflow for the loaded program.
//!
//! The map itself is measured and owned by [`crate::probe`]. This module turns a
//! completed map into one static G-code rewrite with
//! [`ghs_gcode::apply_height_compensation`], keeps the exact original source,
//! and records the applied program so the result sheet can show transform stats
//! and offer Revert. It holds no GPUI types and never sends a command, so the
//! gating, frame checks and source preservation are testable without a window.
//!
//! # Source preservation
//!
//! The app loads demo and file programs through
//! `ghs_engine::Command::LoadProgram`, which carries the exact source string.
//! [`HeightMapCompensation::remember_exact_source`] captures it before the
//! engine parses the program. When only the parsed `Program` is available (for
//! example a future caller that does not pre-register its source), the model
//! rebuilds the text by joining [`ghs_gcode::ProgramLine::source`] with `\n`.
//! That fallback normalises line endings, drops a UTF-8 BOM and may add or drop
//! a final newline; it is still a valid nominal program, never a previously
//! compensated one, because Apply is refused while an application is active.
//!
//! # Safety
//!
//! Apply is refused unless the measurement run completed with every cell valid,
//! the link is ready, the machine is Idle, no job is active, `$#` offsets were
//! reported, and the live WCS/G92/TLO match the snapshot taken when the grid was
//! measured. The rewrite uses [`OutsideMap::Reject`], so a move outside the map
//! fails the whole transform instead of running uncompensated. Nothing here
//! starts a job: the only command an Apply or Revert produces is
//! [`ghs_engine::Command::LoadProgram`].

use ghs_engine::{Axis as CoreAxis, Command, MachineState, Position, Program, Wcs};
use ghs_gcode::{
    HeightCompensationOptions, HeightCompensationStats, HeightCompensationWarning, OutsideMap,
    apply_height_compensation,
};
use ghs_probe::HeightMap;

use crate::probe::{ProbeContext, ProbePhase, ProbeRun, ProbeSetup, format_mm};

/// Suffix appended to the original program name after a successful Apply.
pub const HEIGHT_MAP_PROGRAM_SUFFIX: &str = "[height map]";

/// Two offset values closer than this are the same frame.
///
/// `$#` reports three decimals and the snapshot path does not re-quantise, so
/// this only absorbs floating-point round-trips. A real frame change is far
/// larger than one nanometre.
pub const FRAME_TOLERANCE_MM: f64 = 1.0e-6;

/// Original program text kept for Revert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSource {
    /// Program name supplied with `LoadProgram`.
    pub name: String,
    /// Complete program text.
    pub source: String,
    /// `true` when the app captured the exact source it sent; `false` when the
    /// text was rebuilt from parsed source lines.
    pub exact: bool,
}

impl ProgramSource {
    /// Rebuild a program from its parsed lines.
    ///
    /// This is the fallback when the exact source was not registered. See the
    /// module documentation for what the reconstruction does not preserve.
    #[must_use]
    pub fn from_program(name: &str, program: &Program) -> Self {
        let mut source = String::new();
        for line in &program.lines {
            source.push_str(&line.source);
            source.push('\n');
        }
        Self {
            name: name.to_string(),
            source,
            exact: false,
        }
    }
}

/// The offset frame a height map belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetFrame {
    /// Active work coordinate system.
    pub wcs: Wcs,
    /// Stored `$#` value for that WCS, in millimetres.
    pub wcs_value_mm: Position,
    /// `$#` G92 offset, in millimetres.
    pub g92_mm: Position,
    /// Active tool-length offset Z, in millimetres.
    pub tlo_z_mm: f64,
}

impl OffsetFrame {
    /// Snapshot from the setup that built a probe grid.
    #[must_use]
    pub fn from_setup(setup: &ProbeSetup) -> Self {
        Self {
            wcs: setup.params.wcs,
            wcs_value_mm: setup.wcs_value_mm,
            g92_mm: setup.g92_mm,
            tlo_z_mm: setup.tlo_z_mm,
        }
    }

    /// Snapshot from live preflight context.
    #[must_use]
    pub fn from_context(context: &ProbeContext) -> Self {
        Self {
            wcs: context.wcs,
            wcs_value_mm: context.wcs_value_mm,
            g92_mm: context.g92_mm,
            tlo_z_mm: context.tlo_z_mm,
        }
    }

    /// Total offset from work to machine coordinates: `WCS + G92 + TLO Z`.
    #[must_use]
    pub fn offset_mm(&self) -> Position {
        let mut tlo = Position::zero();
        tlo.set(CoreAxis::Z, self.tlo_z_mm);
        self.wcs_value_mm + self.g92_mm + tlo
    }

    /// Differences between the measured frame (`self`) and the live frame.
    ///
    /// Returns one explicit message per changed element, so the result sheet
    /// can say exactly why Apply is refused instead of only "frame changed".
    #[must_use]
    pub fn mismatches(&self, live: &Self) -> Vec<String> {
        let mut messages = Vec::new();
        if self.wcs != live.wcs {
            messages.push(format!(
                "the active WCS changed from {} to {} since the grid was measured",
                self.wcs.as_str(),
                live.wcs.as_str()
            ));
        }
        if !position_close(self.wcs_value_mm, live.wcs_value_mm) {
            messages.push(format!(
                "the {} WCS offset changed from X{} Y{} Z{} to X{} Y{} Z{} mm since the \
                 grid was measured",
                self.wcs.as_str(),
                format_mm(self.wcs_value_mm.get(CoreAxis::X)),
                format_mm(self.wcs_value_mm.get(CoreAxis::Y)),
                format_mm(self.wcs_value_mm.get(CoreAxis::Z)),
                format_mm(live.wcs_value_mm.get(CoreAxis::X)),
                format_mm(live.wcs_value_mm.get(CoreAxis::Y)),
                format_mm(live.wcs_value_mm.get(CoreAxis::Z)),
            ));
        }
        if !position_close(self.g92_mm, live.g92_mm) {
            messages.push(format!(
                "the G92 offset changed from X{} Y{} Z{} to X{} Y{} Z{} mm since the \
                 grid was measured",
                format_mm(self.g92_mm.get(CoreAxis::X)),
                format_mm(self.g92_mm.get(CoreAxis::Y)),
                format_mm(self.g92_mm.get(CoreAxis::Z)),
                format_mm(live.g92_mm.get(CoreAxis::X)),
                format_mm(live.g92_mm.get(CoreAxis::Y)),
                format_mm(live.g92_mm.get(CoreAxis::Z)),
            ));
        }
        if (self.tlo_z_mm - live.tlo_z_mm).abs() > FRAME_TOLERANCE_MM {
            messages.push(format!(
                "the tool-length offset changed from Z{} to Z{} mm since the grid was measured",
                format_mm(self.tlo_z_mm),
                format_mm(live.tlo_z_mm),
            ));
        }
        messages
    }
}

/// The measurement side of one Apply decision.
#[derive(Clone, Debug)]
pub struct HeightMapSnapshot {
    /// Measured map; missing cells stay invalid.
    pub map: HeightMap,
    /// The run completed and every cell is valid.
    pub complete: bool,
    /// Offset frame recorded when the grid was measured.
    pub frame: OffsetFrame,
}

impl HeightMapSnapshot {
    /// Build a snapshot from a finished height-map run.
    ///
    /// Returns `None` before the first cell exists. A partial or failed run is
    /// still returned with `complete: false` so the sheet can show why Apply is
    /// unavailable.
    #[must_use]
    pub fn from_run(run: &ProbeRun) -> Option<Self> {
        let map = run.map.clone()?;
        Some(Self {
            complete: run.phase == ProbePhase::Complete && map.is_complete(),
            frame: OffsetFrame::from_setup(&run.setup),
            map,
        })
    }

    /// Cells without a valid measurement.
    #[must_use]
    pub fn missing_cells(&self) -> usize {
        self.map.missing_count()
    }
}

/// The live machine side of one Apply decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightApplyGate {
    /// The engine handshake completed.
    pub ready: bool,
    /// Current controller state.
    pub machine_state: MachineState,
    /// A job runner is feeding or paused.
    pub job_active: bool,
    /// `$#` has answered at least once this session.
    pub offsets_reported: bool,
    /// Current offset frame.
    pub frame: OffsetFrame,
}

impl HeightApplyGate {
    /// Snapshot from live preflight context.
    #[must_use]
    pub fn from_context(context: &ProbeContext) -> Self {
        Self {
            ready: context.ready,
            machine_state: context.machine_state,
            job_active: context.job_active,
            offsets_reported: context.offsets_reported,
            frame: OffsetFrame::from_context(context),
        }
    }
}

/// One engine load produced by Apply or Revert.
///
/// The type deliberately carries no Start action: the workflow only ever loads
/// a program for review. This is the exact command the shell sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeightProgramLoad {
    /// Name shown in the job header.
    pub name: String,
    /// Complete program text.
    pub source: String,
    /// `true` for a compensated program, `false` for the original.
    pub compensated: bool,
}

impl HeightProgramLoad {
    /// The only engine command this workflow produces.
    #[must_use]
    pub fn command(&self) -> Command {
        Command::LoadProgram {
            name: self.name.clone(),
            source: self.source.clone(),
        }
    }
}

/// A validated, transformed program waiting for the operator's confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightApplyPreview {
    /// Compensated program name, `<original> [height map]`.
    pub program_name: String,
    /// Original program kept for Revert.
    pub original: ProgramSource,
    /// Total map cells.
    pub cells: usize,
    /// Cells with a valid measurement.
    pub valid_cells: usize,
    /// Grid column spacing, in millimetres.
    pub step_x_mm: f64,
    /// Grid row spacing, in millimetres.
    pub step_y_mm: f64,
    /// Subdivision step passed to the transform, in millimetres.
    pub max_step_mm: f64,
    /// Work-offset XY used for the map lookup, in millimetres.
    pub work_offset_xy_mm: [f64; 2],
    /// Rewrite counters.
    pub stats: HeightCompensationStats,
    /// Non-fatal warnings from the rewrite.
    pub warnings: Vec<HeightCompensationWarning>,
    /// Rewritten program text.
    pub compensated_source: String,
}

impl HeightApplyPreview {
    /// The program load for this preview.
    #[must_use]
    pub fn load(&self) -> HeightProgramLoad {
        HeightProgramLoad {
            name: self.program_name.clone(),
            source: self.compensated_source.clone(),
            compensated: true,
        }
    }

    /// Every emitted motion endpoint received a map value.
    #[must_use]
    pub fn is_fully_covered(&self) -> bool {
        self.stats.uncompensated_motions == 0
            && self.stats.output_motion_blocks == self.stats.compensated_motions
    }
}

/// What the result sheet says about the Apply workflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightMapApplyStatus {
    /// No complete map, or no original program to compensate.
    MeasurementOnly,
    /// A complete map is available and Apply passed its gates.
    ReadyToApply,
    /// The compensated program is loaded.
    Applied,
    /// The original program was restored.
    Reverted,
}

impl HeightMapApplyStatus {
    /// Short status text for the result sheet.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MeasurementOnly => "MEASUREMENT ONLY",
            Self::ReadyToApply => "READY TO APPLY",
            Self::Applied => "APPLIED",
            Self::Reverted => "REVERTED",
        }
    }
}

/// Apply/Revert state retained across probe cards and job events.
///
/// `original` is the dedicated source field: it is cleared by
/// [`Self::reset`] for a new session and replaced when a different program
/// loads. The measured map is not owned here; the caller passes a
/// [`HeightMapSnapshot`] for each decision.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeightMapCompensation {
    /// Verbatim source registered by the app before `LoadProgram`.
    exact_source: Option<ProgramSource>,
    /// Source of the current job, kept while it is the part program.
    ///
    /// A height-map run promotes this to `original` before the probe program
    /// replaces the job. A new program clears it.
    job_source: Option<ProgramSource>,
    /// Original program kept for Revert.
    original: Option<ProgramSource>,
    /// First work coordinate system named by the original program.
    ///
    /// Cached when the original is adopted so the result sheet's per-render
    /// gate checks never re-parse the program.
    original_wcs: Option<Wcs>,
    /// Transform awaiting explicit confirmation.
    pending: Option<HeightApplyPreview>,
    /// Currently applied compensated program.
    applied: Option<HeightApplyPreview>,
    /// Last explicit failure, shown on the result sheet.
    last_error: Option<String>,
    /// The last completed action was Revert.
    reverted: bool,
}

impl HeightMapCompensation {
    /// Clear everything for a new controller session.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Register the exact source the app is about to send with `LoadProgram`.
    ///
    /// The engine's `JobLoaded` event carries only the parsed program, so this
    /// is the only point where a demo or file program's exact text is known.
    pub fn remember_exact_source(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.exact_source = Some(ProgramSource {
            name: name.into(),
            source: source.into(),
            exact: true,
        });
    }

    /// Discard an exact source whose `LoadProgram` the engine rejected.
    ///
    /// The source is registered before the command for the `JobLoaded` event.
    /// When the engine refuses the parse, no event arrives and the previous
    /// program stays loaded; leaving the failed text registered could let a
    /// later same-name load adopt bytes that never ran.
    pub fn discard_exact_source(&mut self, name: &str) {
        if self
            .exact_source
            .as_ref()
            .is_some_and(|source| source.name == name)
        {
            self.exact_source = None;
        }
    }

    /// Adopt a `JobLoaded` program as the original, unless it belongs to the
    /// probe workflow or is the compensated program Apply just loaded.
    ///
    /// A different program is a new setup: any applied map and its Revert state
    /// are cleared. The probe program and the compensated program are
    /// intermediate loads and keep the original source intact.
    pub fn on_job_loaded(&mut self, name: &str, program: &Program, probe_program: bool) {
        if probe_program {
            return;
        }
        // A file with the same display name can come from another directory.
        // The exact source registered before the load is the only reliable
        // identity; never let it inherit an original or applied program just
        // because the base name matches.
        let exact_is_new = self
            .exact_source
            .as_ref()
            .is_some_and(|exact| exact.name == name)
            && (self.applied.as_ref().is_some_and(|applied| {
                applied.program_name == name
                    && applied.compensated_source != self.exact_source_text(name)
            }) || self.original.as_ref().is_some_and(|original| {
                original.name == name && original.source != self.exact_source_text(name)
            }));
        if !exact_is_new
            && self
                .applied
                .as_ref()
                .is_some_and(|applied| applied.program_name == name)
        {
            return;
        }
        if name.ends_with(HEIGHT_MAP_PROGRAM_SUFFIX) {
            // Our own Apply output must never become an original again, which
            // would let a second transform stack on top of the first.
            self.last_error = Some(format!(
                "refusing to adopt '{name}' as an original program; load the \
                 uncompensated source before applying a height map"
            ));
            return;
        }
        if !exact_is_new
            && self
                .original
                .as_ref()
                .is_some_and(|original| original.name == name)
        {
            // A reload of the same original (for example after Revert).
            self.pending = None;
            return;
        }
        // A different program is a new setup: the map belongs to the previous
        // one point. Clear both the applied state and the original source; a
        // new height-map run promotes the new job source.
        self.pending = None;
        self.applied = None;
        self.reverted = false;
        self.last_error = None;
        self.original = None;
        self.original_wcs = None;
        self.job_source = match self.exact_source.take() {
            Some(exact) if exact.name == name => Some(exact),
            Some(_) => Some(ProgramSource::from_program(name, program)),
            None => Some(ProgramSource::from_program(name, program)),
        };
    }

    fn exact_source_text(&self, name: &str) -> &str {
        self.exact_source
            .as_ref()
            .filter(|source| source.name == name)
            .map_or("", |source| source.source.as_str())
    }

    /// Snapshot the current part program as the original before a probe run
    /// replaces the loaded job.
    ///
    /// A previous application is cleared: the compensated program is about to
    /// be replaced. The original snapshot is kept so the result sheet can still
    /// Revert or apply the new map.
    pub fn on_probe_run_started(&mut self) {
        self.pending = None;
        self.applied = None;
        self.reverted = false;
        self.last_error = None;
        if self.original.is_none()
            && let Some(source) = self.job_source.clone()
        {
            self.original_wcs = selected_wcs(&source.source);
            self.original = Some(source);
        }
    }

    /// Adopt the loaded part program as the original for a saved-map Apply.
    ///
    /// Selecting a stored map only views it. This method is called by the
    /// explicit Apply action (never by load) and records the current program
    /// source so [`Self::request_apply`] can rewrite it. It clears no applied
    /// state and keeps any pending preview untouched.
    ///
    /// Returns `true` when an original source is available afterwards.
    pub fn adopt_current_program_for_map(&mut self) -> bool {
        if self.original.is_none()
            && let Some(source) = self.job_source.clone()
        {
            self.original_wcs = selected_wcs(&source.source);
            self.original = Some(source);
        }
        self.original.is_some()
    }

    /// Original program kept for Revert.
    #[must_use]
    pub fn original(&self) -> Option<&ProgramSource> {
        self.original.as_ref()
    }

    /// Transform waiting for confirmation.
    #[must_use]
    pub fn pending(&self) -> Option<&HeightApplyPreview> {
        self.pending.as_ref()
    }

    /// Currently applied compensated program.
    #[must_use]
    pub fn applied(&self) -> Option<&HeightApplyPreview> {
        self.applied.as_ref()
    }

    /// Last explicit Apply/Revert error.
    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Whether the current job is the compensated program.
    #[must_use]
    pub fn is_applied_for(&self, job_name: &str) -> bool {
        self.applied
            .as_ref()
            .is_some_and(|applied| applied.program_name == job_name)
    }

    /// Current sheet status.
    #[must_use]
    pub fn status(&self, snapshot: Option<&HeightMapSnapshot>) -> HeightMapApplyStatus {
        self.status_for(snapshot.is_some_and(|snapshot| snapshot.complete))
    }

    /// Status from a pre-computed completeness flag.
    ///
    /// The shell uses this in the footer, where cloning the map just to read
    /// the status would be wasteful.
    #[must_use]
    pub fn status_for(&self, complete: bool) -> HeightMapApplyStatus {
        if self.applied.is_some() {
            HeightMapApplyStatus::Applied
        } else if self.reverted {
            HeightMapApplyStatus::Reverted
        } else if complete && self.original.is_some() {
            HeightMapApplyStatus::ReadyToApply
        } else {
            HeightMapApplyStatus::MeasurementOnly
        }
    }

    /// Every reason Apply is refused.
    ///
    /// The list is empty only when the map is complete, the link is ready, the
    /// machine is Idle, no job is active, `$#` was reported and the live
    /// WCS/G92/TLO match the measurement snapshot.
    #[must_use]
    pub fn blockers(&self, snapshot: &HeightMapSnapshot, gate: &HeightApplyGate) -> Vec<String> {
        let mut blockers = Vec::new();
        if !snapshot.complete {
            if snapshot.missing_cells() > 0 {
                blockers.push(format!(
                    "the height map has {} invalid cell(s); re-probe them before applying",
                    snapshot.missing_cells()
                ));
            } else {
                blockers.push("the height-map run has not completed".to_string());
            }
        }
        if !gate.ready {
            blockers.push("the controller link is not ready".to_string());
        }
        if gate.machine_state != MachineState::Idle {
            blockers.push(format!(
                "the machine is not Idle (state: {:?}); wait for Idle before applying",
                gate.machine_state
            ));
        }
        if gate.job_active {
            blockers.push("a job is active; stop it before applying".to_string());
        }
        if !gate.offsets_reported {
            blockers
                .push("the offsets have not been reported; refresh $# before applying".to_string());
        }
        blockers.extend(snapshot.frame.mismatches(&gate.frame));
        if self
            .original
            .as_ref()
            .is_none_or(|original| original.source.trim().is_empty())
        {
            blockers.push(
                "no original program source is available to compensate; load \
                 the part program before measuring"
                    .to_string(),
            );
        } else if let Some(program_wcs) = self.original_wcs
            && program_wcs != snapshot.frame.wcs
        {
            blockers.push(format!(
                "the program selects {} but the grid was measured in {}; apply \
                 it in the measured frame",
                program_wcs.as_str(),
                snapshot.frame.wcs.as_str()
            ));
        }
        if self.applied.is_some() {
            blockers.push("the height map is already applied; revert it first".to_string());
        }
        blockers
    }

    /// Validate, transform, and cache a preview for the confirmation step.
    ///
    /// The job is untouched. A transform error is stored in [`Self::last_error`]
    /// and returned as the only blocker; no load command exists in that case.
    ///
    /// # Errors
    ///
    /// Returns every gate blocker, or the transform error text.
    pub fn request_apply(
        &mut self,
        snapshot: &HeightMapSnapshot,
        gate: &HeightApplyGate,
    ) -> Result<(), Vec<String>> {
        let blockers = self.blockers(snapshot, gate);
        if !blockers.is_empty() {
            self.pending = None;
            return Err(blockers);
        }
        let Some(original) = self.original.clone() else {
            self.pending = None;
            return Err(vec!["no original program source is available".to_string()]);
        };
        let grid = snapshot.map.grid();
        let options = HeightCompensationOptions {
            // TLO is Z-only; the map lookup is an XY work translation.
            work_offset_xy_mm: [
                original_frame_x(&snapshot.frame),
                original_frame_y(&snapshot.frame),
            ],
            max_step_mm: grid.step_x_mm.min(grid.step_y_mm),
            outside: OutsideMap::Reject,
        };
        let transform = apply_height_compensation(&original.source, &options, |x_mm, y_mm| {
            snapshot.map.compensation_at(x_mm, y_mm)
        });
        match transform {
            Ok(result) => {
                self.pending = Some(HeightApplyPreview {
                    program_name: format!("{} {HEIGHT_MAP_PROGRAM_SUFFIX}", original.name),
                    original,
                    cells: snapshot.map.len(),
                    valid_cells: snapshot.map.valid_count(),
                    step_x_mm: grid.step_x_mm,
                    step_y_mm: grid.step_y_mm,
                    max_step_mm: options.max_step_mm,
                    work_offset_xy_mm: options.work_offset_xy_mm,
                    stats: result.stats,
                    warnings: result.warnings,
                    compensated_source: result.source,
                });
                self.last_error = None;
                Ok(())
            }
            Err(error) => {
                self.pending = None;
                let message = format!("Height map not applied: {error}");
                self.last_error = Some(message.clone());
                Err(vec![message])
            }
        }
    }

    /// Cancel the pending confirmation.
    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// Record a successful `LoadProgram` for the preview.
    pub fn mark_applied(&mut self, preview: HeightApplyPreview) {
        self.pending = None;
        self.applied = Some(preview);
        self.reverted = false;
        self.last_error = None;
    }

    /// Program load that restores the original.
    ///
    /// # Errors
    ///
    /// Returns a message when no original source is available.
    pub fn revert_load(&self) -> Result<HeightProgramLoad, String> {
        let original = self
            .original
            .as_ref()
            .ok_or_else(|| "no original program source is available to restore".to_string())?;
        if original.source.trim().is_empty() {
            return Err("the original program source is empty".to_string());
        }
        Ok(HeightProgramLoad {
            name: original.name.clone(),
            source: original.source.clone(),
            compensated: false,
        })
    }

    /// Record a successful Revert load.
    pub fn mark_reverted(&mut self) {
        self.applied = None;
        self.pending = None;
        self.reverted = true;
        self.last_error = None;
    }

    /// Record a failed Apply or Revert command.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }
}

/// Work-offset X used for the map lookup, from one fixed frame.
fn original_frame_x(frame: &OffsetFrame) -> f64 {
    frame.wcs_value_mm.get(CoreAxis::X) + frame.g92_mm.get(CoreAxis::X)
}

/// Work-offset Y used for the map lookup, from one fixed frame.
fn original_frame_y(frame: &OffsetFrame) -> f64 {
    frame.wcs_value_mm.get(CoreAxis::Y) + frame.g92_mm.get(CoreAxis::Y)
}

/// First work coordinate system named by a program, if any.
///
/// The transform requires one fixed work offset. This check makes sure the
/// program's own `G54`-`G59.3` word names the frame the map was measured in,
/// rather than silently applying the measured offset to another system.
fn selected_wcs(source: &str) -> Option<Wcs> {
    let program = ghs_gcode::parse_program(source).ok()?;
    for line in &program.lines {
        for word in &line.words {
            if word.letter != 'G' {
                continue;
            }
            if let Some(wcs) = Wcs::parse(&format!("G{}", word.value)) {
                return Some(wcs);
            }
        }
    }
    None
}

/// Compare the XY (and Z for TLO) elements of a position.
fn position_close(a: Position, b: Position) -> bool {
    [CoreAxis::X, CoreAxis::Y, CoreAxis::Z]
        .iter()
        .all(|axis| (a.get(*axis) - b.get(*axis)).abs() <= FRAME_TOLERANCE_MM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_engine::Program;
    use ghs_probe::{HeightMapGrid, XyPoint};

    const SOURCE: &str = "\
G21 G90 G54 G17
G0 Z5
G0 X0 Y0
M3 S12000
G1 Z-1 F200
G1 X40 F800
G2 X40 Y40 I0 J20
G1 X0
G1 Y0
M5
M30
";

    fn measured_frame() -> OffsetFrame {
        OffsetFrame {
            wcs: Wcs::G54,
            wcs_value_mm: Position::xyz(5.0, 6.0, 30.0),
            g92_mm: Position::xyz(1.0, 2.0, 0.0),
            tlo_z_mm: 0.5,
        }
    }

    fn gate() -> HeightApplyGate {
        HeightApplyGate {
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            offsets_reported: true,
            frame: measured_frame(),
        }
    }

    fn complete_grid() -> HeightMapGrid {
        // 4x4 at 20 mm covers machine X/Y 6..48 with the measured frame.
        HeightMapGrid {
            origin_mm: XyPoint::new(-10.0, -10.0),
            step_x_mm: 20.0,
            step_y_mm: 20.0,
            columns: 4,
            rows: 4,
            surface_z_mm: 0.0,
        }
    }

    fn complete_map() -> HeightMapSnapshot {
        let grid = complete_grid();
        let samples = (0..grid.point_count())
            .map(|index| Some((index as f64) * 0.01))
            .collect();
        let map = HeightMap::from_samples(grid, samples).expect("complete map");
        HeightMapSnapshot {
            complete: true,
            frame: measured_frame(),
            map,
        }
    }

    fn model_with_original() -> HeightMapCompensation {
        let mut model = HeightMapCompensation::default();
        model.remember_exact_source("part.nc", SOURCE);
        let program: Program = ghs_gcode::parse_program(SOURCE).expect("source parses");
        model.on_job_loaded("part.nc", &program, false);
        model.on_probe_run_started();
        model
    }

    #[test]
    fn complete_map_apply_builds_one_load_with_stats() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::ReadyToApply
        );
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");

        let preview = model.pending().expect("confirmation pending");
        assert_eq!(preview.program_name, "part.nc [height map]");
        assert_eq!(preview.max_step_mm, 20.0);
        assert_eq!(preview.work_offset_xy_mm, [6.0, 8.0]);
        assert_eq!(preview.valid_cells, 16);
        assert!(preview.stats.input_lines >= 10);
        assert!(preview.stats.split_motions > 0, "the 40 mm moves split");
        assert_eq!(preview.stats.arc_motions, 1);
        assert_eq!(preview.stats.uncompensated_motions, 0);
        assert!(preview.is_fully_covered());
        assert!(!preview.compensated_source.is_empty());

        let load = preview.load();
        assert_eq!(load.name, "part.nc [height map]");
        assert!(load.compensated);
        model.mark_applied(preview.clone());
        assert!(model.is_applied_for("part.nc [height map]"));
        assert!(!model.is_applied_for("part.nc"));
        assert_eq!(model.status(Some(&snapshot)), HeightMapApplyStatus::Applied);
    }

    #[test]
    fn incomplete_map_is_refused_and_never_pending() {
        let mut model = model_with_original();
        let grid = complete_grid();
        let mut samples = (0..grid.point_count())
            .map(|index| Some((index as f64) * 0.01))
            .collect::<Vec<_>>();
        samples[5] = None;
        let map = HeightMap::from_samples(grid, samples).expect("partial map");
        let snapshot = HeightMapSnapshot {
            complete: false,
            frame: measured_frame(),
            map,
        };

        let blockers = model
            .request_apply(&snapshot, &gate())
            .expect_err("a partial map is refused");
        assert!(
            blockers.iter().any(|text| text.contains("1 invalid cell")),
            "unexpected blockers: {blockers:?}"
        );
        assert!(model.pending().is_none());
        assert!(model.applied().is_none());
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::MeasurementOnly
        );
        assert_eq!(
            model.original().map(|source| source.source.as_str()),
            Some(SOURCE)
        );
    }

    #[test]
    fn wcs_g92_and_tlo_mismatches_are_named_and_refused() {
        let snapshot = complete_map();
        let mut cases = Vec::new();

        let mut moved_wcs = gate();
        moved_wcs.frame.wcs = Wcs::G55;
        cases.push((moved_wcs, "WCS changed"));

        let mut moved_wcs_value = gate();
        moved_wcs_value.frame.wcs_value_mm = Position::xyz(5.5, 6.0, 30.0);
        cases.push((moved_wcs_value, "WCS offset changed"));

        let mut moved_g92 = gate();
        moved_g92.frame.g92_mm = Position::xyz(1.0, 2.5, 0.0);
        cases.push((moved_g92, "G92 offset changed"));

        let mut moved_tlo = gate();
        moved_tlo.frame.tlo_z_mm = 0.75;
        cases.push((moved_tlo, "tool-length offset changed"));

        for (bad_gate, expected) in cases {
            let mut model = model_with_original();
            let blockers = model
                .request_apply(&snapshot, &bad_gate)
                .expect_err("frame mismatch refused");
            assert!(
                blockers.iter().any(|text| text.contains(expected)),
                "expected '{expected}', got {blockers:?}"
            );
            assert!(model.pending().is_none());
            assert!(model.applied().is_none());
        }
    }

    #[test]
    fn transform_error_leaves_the_job_and_original_unchanged() {
        let mut model = HeightMapCompensation::default();
        let bad_source = "G21 G90\nG53 G0 X0\nM30\n";
        model.remember_exact_source("bad.nc", bad_source);
        let program = ghs_gcode::parse_program(bad_source).expect("source parses");
        model.on_job_loaded("bad.nc", &program, false);
        model.on_probe_run_started();

        let snapshot = complete_map();
        let blockers = model
            .request_apply(&snapshot, &gate())
            .expect_err("G53 is refused by the transform");
        assert!(
            blockers.iter().any(|text| text.contains("G53")),
            "unexpected blockers: {blockers:?}"
        );
        assert!(model.pending().is_none());
        assert!(model.applied().is_none());
        assert_eq!(
            model.original().map(|source| source.source.as_str()),
            Some(bad_source)
        );
        assert!(model.last_error().is_some_and(|text| text.contains("G53")));
    }

    #[test]
    fn outside_map_is_rejected_instead_of_left_uncompensated() {
        let mut model = model_with_original();
        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(0.0, 0.0),
            step_x_mm: 10.0,
            step_y_mm: 10.0,
            columns: 2,
            rows: 2,
            surface_z_mm: 0.0,
        };
        let map = HeightMap::from_samples(grid, vec![Some(0.0); 4]).expect("small map");
        let snapshot = HeightMapSnapshot {
            complete: true,
            frame: measured_frame(),
            map,
        };
        let blockers = model
            .request_apply(&snapshot, &gate())
            .expect_err("outside moves reject the transform");
        assert!(
            blockers
                .iter()
                .any(|text| text.contains("outside the height map")),
            "unexpected blockers: {blockers:?}"
        );
        assert!(model.applied().is_none());
    }

    #[test]
    fn revert_restores_the_exact_original_and_clears_applied_state() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        let preview = model.pending().expect("pending").clone();
        model.mark_applied(preview);

        let revert = model.revert_load().expect("original is available");
        assert_eq!(revert.name, "part.nc");
        assert_eq!(revert.source, SOURCE);
        assert!(!revert.compensated);
        model.mark_reverted();

        assert!(model.applied().is_none());
        assert!(!model.is_applied_for("part.nc [height map]"));
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::Reverted
        );
        // Revert is a load, never a start, and Apply is available again.
        assert!(matches!(
            revert.command(),
            Command::LoadProgram { ref name, .. } if name == "part.nc"
        ));
    }

    #[test]
    fn apply_only_ever_produces_a_load_command() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        let load = model.pending().expect("pending").load();
        let command = load.command();
        assert!(matches!(
            command,
            Command::LoadProgram { ref name, .. } if name == "part.nc [height map]"
        ));
        assert!(!matches!(command, Command::StartProgram { .. }));
        assert_eq!(load.name, format!("part.nc {HEIGHT_MAP_PROGRAM_SUFFIX}"));
    }

    #[test]
    fn reconstructed_source_is_marked_and_usable_once() {
        let mut model = HeightMapCompensation::default();
        let program =
            ghs_gcode::parse_program("G21 G90\nG1 X10 F100\nM30\n").expect("source parses");
        model.on_job_loaded("parsed.nc", &program, false);
        model.on_probe_run_started();
        let original = model.original().expect("original reconstructed");
        assert!(!original.exact);
        assert!(original.source.contains("G1 X10"));

        let grid = HeightMapGrid {
            origin_mm: XyPoint::new(0.0, 0.0),
            step_x_mm: 10.0,
            step_y_mm: 10.0,
            columns: 2,
            rows: 2,
            surface_z_mm: 0.0,
        };
        let map = HeightMap::from_samples(grid, vec![Some(0.0); 4]).expect("map");
        let snapshot = HeightMapSnapshot {
            complete: true,
            frame: OffsetFrame {
                wcs: Wcs::G54,
                wcs_value_mm: Position::zero(),
                g92_mm: Position::zero(),
                tlo_z_mm: 0.0,
            },
            map,
        };
        let mut gate = gate();
        gate.frame = snapshot.frame;
        model
            .request_apply(&snapshot, &gate)
            .expect("reconstructed source transforms");
        let preview = model.pending().expect("pending").clone();
        assert_eq!(preview.stats.uncompensated_motions, 0);
        model.mark_applied(preview);
        assert!(model.applied().is_some());
        assert!(
            model.request_apply(&snapshot, &gate).is_err(),
            "a second Apply must wait for Revert"
        );
    }

    #[test]
    fn new_program_clears_the_applied_state_and_revert_marker() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        model.mark_applied(model.pending().expect("pending").clone());
        assert!(model.applied().is_some());

        let next = ghs_gcode::parse_program("G21\nG0 X1\nM30\n").expect("next program");
        model.on_job_loaded("next.nc", &next, false);
        assert!(model.applied().is_none());
        assert!(!model.is_applied_for("part.nc [height map]"));
        assert!(
            model.original().is_none(),
            "a new program must clear the original until a new map run"
        );
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::MeasurementOnly
        );
        model.on_probe_run_started();
        assert_eq!(
            model.original().map(|source| source.name.as_str()),
            Some("next.nc")
        );
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::ReadyToApply
        );
    }

    #[test]
    fn same_name_different_source_is_a_new_program() {
        let mut model = model_with_original();
        assert!(model.original().is_some());

        let next_source = "G21\nG0 X2\nM30\n";
        model.remember_exact_source("part.nc", next_source);
        let next = ghs_gcode::parse_program(next_source).expect("next program");
        model.on_job_loaded("part.nc", &next, false);
        assert!(
            model.original().is_none(),
            "same base name from another directory must not inherit the old original"
        );

        model.on_probe_run_started();
        assert_eq!(
            model.original().map(|source| source.source.as_str()),
            Some(next_source)
        );
    }

    #[test]
    fn probe_program_and_compensated_program_keep_the_original() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        let preview = model.pending().expect("pending").clone();
        let compensated = ghs_gcode::parse_program(&preview.compensated_source)
            .expect("compensated program parses");
        model.mark_applied(preview.clone());
        model.on_job_loaded(&preview.program_name, &compensated, false);
        assert!(model.original().is_some());
        assert!(model.is_applied_for(&preview.program_name));

        let probe = ghs_gcode::parse_program("G21\nG0 X1\nM30\n").expect("probe program");
        model.on_job_loaded("height-map", &probe, true);
        assert_eq!(
            model.original().map(|source| source.name.as_str()),
            Some("part.nc")
        );
    }

    #[test]
    fn program_wcs_must_match_the_measured_frame() {
        let mut model = HeightMapCompensation::default();
        let source = "G21 G90 G55 G17\nG1 X10 Y10 F100\nM30\n";
        model.remember_exact_source("g55.nc", source);
        let program = ghs_gcode::parse_program(source).expect("source parses");
        model.on_job_loaded("g55.nc", &program, false);
        model.on_probe_run_started();

        let snapshot = complete_map();
        let blockers = model
            .request_apply(&snapshot, &gate())
            .expect_err("a program in another WCS is refused");
        assert!(
            blockers
                .iter()
                .any(|text| text.contains("program selects G55")),
            "unexpected blockers: {blockers:?}"
        );
        assert!(model.pending().is_none());
        assert!(model.applied().is_none());
    }

    #[test]
    fn starting_a_probe_run_clears_applied_but_keeps_original() {
        let mut model = model_with_original();
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        model.mark_applied(model.pending().expect("pending").clone());
        assert!(model.applied().is_some());

        model.on_probe_run_started();
        assert!(model.applied().is_none());
        assert_eq!(
            model.original().map(|source| source.name.as_str()),
            Some("part.nc")
        );
        assert_eq!(
            model.status(Some(&snapshot)),
            HeightMapApplyStatus::ReadyToApply
        );
    }

    #[test]
    fn app_state_marks_only_the_compensated_job() {
        use std::sync::Arc;

        let mut state = crate::state::AppState::new();
        state.height_map = model_with_original();
        let snapshot = complete_map();
        state
            .height_map
            .request_apply(&snapshot, &gate())
            .expect("apply is legal");
        let preview = state.height_map.pending().expect("pending").clone();
        state.height_map.mark_applied(preview.clone());
        let compensated =
            ghs_gcode::parse_program(&preview.compensated_source).expect("compensated parses");
        state
            .job
            .loaded(preview.program_name.clone(), Arc::new(compensated), 5);
        assert!(state.job_compensated(), "the job marker must be visible");

        let original = ghs_gcode::parse_program(SOURCE).expect("original parses");
        state.job.loaded("part.nc".into(), Arc::new(original), 5);
        assert!(
            !state.job_compensated(),
            "the marker must not stick to the original"
        );
    }

    #[test]
    fn apply_gates_cover_ready_idle_job_and_offsets() {
        let snapshot = complete_map();
        for (label, mutate) in [
            (
                "link",
                (|gate: &mut HeightApplyGate| gate.ready = false) as fn(&mut HeightApplyGate),
            ),
            ("idle", |gate: &mut HeightApplyGate| {
                gate.machine_state = MachineState::Run
            }),
            ("job", |gate: &mut HeightApplyGate| gate.job_active = true),
            ("offsets", |gate: &mut HeightApplyGate| {
                gate.offsets_reported = false
            }),
        ] {
            let mut model = model_with_original();
            let mut gate = gate();
            mutate(&mut gate);
            let blockers = model
                .request_apply(&snapshot, &gate)
                .expect_err("gate refuses");
            assert!(
                !blockers.is_empty(),
                "{label}: a gate must produce a blocker"
            );
            assert!(model.applied().is_none());
        }
    }

    #[test]
    fn a_saved_map_can_use_the_same_gated_apply_after_adopting_the_program() {
        let mut model = HeightMapCompensation::default();
        model.remember_exact_source("part.nc", SOURCE);
        let program: Program = ghs_gcode::parse_program(SOURCE).expect("source parses");
        model.on_job_loaded("part.nc", &program, false);
        assert!(model.original().is_none(), "no probe run has started yet");

        assert!(model.adopt_current_program_for_map());
        assert_eq!(
            model.original().map(|source| source.name.as_str()),
            Some("part.nc")
        );
        let snapshot = complete_map();
        model
            .request_apply(&snapshot, &gate())
            .expect("the saved map applies through the same gates");
        let preview = model.pending().expect("preview").clone();
        assert_eq!(preview.program_name, "part.nc [height map]");
        assert!(matches!(
            preview.load().command(),
            Command::LoadProgram { .. }
        ));
        assert!(
            !matches!(preview.load().command(), Command::StartProgram { .. }),
            "Apply never starts the job"
        );
    }
}
