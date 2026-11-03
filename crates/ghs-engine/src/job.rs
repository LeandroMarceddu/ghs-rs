//! Job runner state machine layered over the M2 protocol session.
//!
//! The runner owns the parsed program, the cursor, the virtual stop flags and
//! the tool-change handshake. It never writes to the transport: the engine
//! runtime asks it for the next line and feeds that line through the same
//! [`Streamer`](ghs_proto::Streamer) and safety gateway as MDI and system
//! commands. Exactly one program line is in flight at a time, so an `ok` can
//! always be mapped back to the source line that produced it.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ghs_core::{JobInfo, JobState};
use ghs_gcode::{Program, ProgramLine, parse_program};

use crate::error::EngineError;
use crate::event::JobPauseReason;

/// Maximum program source accepted by [`JobRunner::load`], in bytes.
///
/// Parsing happens on the engine thread, so the input is bounded to keep a
/// pathological file from stalling telemetry indefinitely.
pub(crate) const MAX_PROGRAM_BYTES: usize = 64 * 1024 * 1024;
/// Maximum source lines accepted by [`JobRunner::load`].
pub(crate) const MAX_PROGRAM_LINES: usize = 2_000_000;
/// Progress events are emitted at most this often unless a lifecycle event
/// forces one.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// Progress is also emitted after every this many resolved lines.
const PROGRESS_LINE_STEP: u32 = 32;
/// How long the controller may take to enter `Tool` after an acknowledged
/// `M6` before the runner reports a graceful failure.
const TOOL_CHANGE_TIMEOUT: Duration = Duration::from_secs(3);

const FLAG_PROGRAM_STOP: u8 = 1 << 0;
const FLAG_OPTIONAL_STOP: u8 = 1 << 1;
const FLAG_TOOL_CHANGE: u8 = 1 << 2;
const FLAG_PROGRAM_END: u8 = 1 << 3;

/// Per-line metadata derived once at load time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineMeta {
    /// Modal tool number after this line.
    pub(crate) tool: u32,
    flags: u8,
}

impl LineMeta {
    /// Returns `true` when the block is `M0`.
    pub(crate) const fn is_program_stop(self) -> bool {
        self.flags & FLAG_PROGRAM_STOP != 0
    }

    /// Returns `true` when the block is `M1`.
    pub(crate) const fn is_optional_stop(self) -> bool {
        self.flags & FLAG_OPTIONAL_STOP != 0
    }

    /// Returns `true` when the block contains `M6`.
    pub(crate) const fn is_tool_change(self) -> bool {
        self.flags & FLAG_TOOL_CHANGE != 0
    }

    /// Returns `true` when the block contains `M2` or `M30`.
    pub(crate) const fn is_program_end(self) -> bool {
        self.flags & FLAG_PROGRAM_END != 0
    }
}

/// A program line that has been written and is waiting for its `ok`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InFlightLine {
    index: usize,
    line: u32,
    meta: LineMeta,
}

/// The result of acknowledging the in-flight program line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AckInfo {
    /// Zero-based index into [`Program::lines`].
    pub(crate) index: usize,
    /// 1-based source line number.
    pub(crate) line: u32,
    /// Per-line metadata.
    pub(crate) meta: LineMeta,
    /// Whether the throttled progress event is due.
    pub(crate) progress_due: bool,
    /// Active job time at the acknowledgement.
    pub(crate) elapsed: Duration,
}

/// A pending manual tool change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingToolChange {
    /// Requested tool number.
    pub(crate) tool: u32,
    /// Source line carrying the `M6`.
    pub(crate) line: u32,
    acknowledged: bool,
    saw_tool_state: bool,
    deadline: Instant,
}

/// Lifecycle of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Empty,
    Loaded,
    Running,
    Paused(JobPauseReason),
    ToolChange,
    Completed,
    Aborted,
    Interrupted,
}

/// Owned job state.
#[derive(Debug, Default)]
pub(crate) struct JobRunner {
    phase: Option<Phase>,
    name: Option<String>,
    program: Option<Arc<Program>>,
    meta: Vec<LineMeta>,
    breakpoints: BTreeSet<u32>,
    cursor: usize,
    completed: u32,
    current_line: u32,
    total_lines: u32,
    in_flight: Option<InFlightLine>,
    run_started: Option<Instant>,
    accumulated: Duration,
    last_progress_at: Option<Instant>,
    last_progress_completed: u32,
    last_breakpoint_hit: Option<u32>,
    optional_stop: bool,
    single_block: bool,
    block_delete: bool,
    active_tool: u32,
    tool_change: Option<PendingToolChange>,
    check: bool,
}

impl JobRunner {
    /// Creates an empty runner.
    pub(crate) fn new() -> Self {
        JobRunner {
            phase: Some(Phase::Empty),
            ..JobRunner::default()
        }
    }

    fn phase(&self) -> Phase {
        self.phase.unwrap_or(Phase::Empty)
    }

    /// Parses, validates and installs a new program.
    pub(crate) fn load(&mut self, name: String, source: &str) -> Result<Arc<Program>, EngineError> {
        if self.is_active() {
            return Err(EngineError::JobState {
                action: "load a program",
                state: self.job_state(),
            });
        }
        if source.len() > MAX_PROGRAM_BYTES {
            return Err(EngineError::InvalidProgram(format!(
                "source is {} bytes; the limit is {MAX_PROGRAM_BYTES}",
                source.len()
            )));
        }
        if source.lines().count() > MAX_PROGRAM_LINES {
            return Err(EngineError::InvalidProgram(format!(
                "source has more than {MAX_PROGRAM_LINES} lines"
            )));
        }
        let program = parse_program(source).map_err(|errors| {
            let message = errors
                .first()
                .map_or_else(|| "program is invalid".to_string(), ToString::to_string);
            EngineError::InvalidProgram(message)
        })?;
        if program.lines.len() > MAX_PROGRAM_LINES {
            return Err(EngineError::InvalidProgram(format!(
                "program has {} lines; the limit is {MAX_PROGRAM_LINES}",
                program.lines.len()
            )));
        }

        let mut meta = Vec::with_capacity(program.lines.len());
        let mut tool = 0u32;
        let mut total = program.lines.len();
        let mut found_end = false;
        for (index, line) in program.lines.iter().enumerate() {
            let mut flags = 0u8;
            for word in &line.words {
                if word.letter == 'T' && word.value.is_finite() && word.value >= 0.0 {
                    tool = word.value as u32;
                } else if word.letter == 'M' && word.value.is_finite() {
                    if code_is(word.value, 0.0) {
                        flags |= FLAG_PROGRAM_STOP;
                    } else if code_is(word.value, 1.0) {
                        flags |= FLAG_OPTIONAL_STOP;
                    } else if code_is(word.value, 6.0) {
                        flags |= FLAG_TOOL_CHANGE;
                    } else if code_is(word.value, 2.0) || code_is(word.value, 30.0) {
                        flags |= FLAG_PROGRAM_END;
                    }
                }
            }
            meta.push(LineMeta { tool, flags });
            if flags & FLAG_PROGRAM_END != 0 && !found_end {
                total = index.saturating_add(1);
                found_end = true;
            }
        }

        let total_lines = u32::try_from(total).map_err(|_| {
            EngineError::InvalidProgram("program line count does not fit in 32 bits".into())
        })?;
        let program = Arc::new(program);
        self.phase = Some(Phase::Loaded);
        self.name = Some(name);
        self.program = Some(Arc::clone(&program));
        self.meta = meta;
        self.breakpoints.clear();
        self.cursor = 0;
        self.completed = 0;
        self.current_line = 0;
        self.total_lines = total_lines;
        self.in_flight = None;
        self.run_started = None;
        self.accumulated = Duration::ZERO;
        self.last_progress_at = None;
        self.last_progress_completed = 0;
        self.last_breakpoint_hit = None;
        self.active_tool = 0;
        self.tool_change = None;
        self.check = false;
        Ok(program)
    }

    /// Whether a program is installed.
    pub(crate) const fn has_program(&self) -> bool {
        self.program.is_some()
    }

    /// Whether the runner is currently feeding lines.
    pub(crate) fn is_running(&self) -> bool {
        matches!(self.phase(), Phase::Running)
    }

    /// Whether the runner is paused by any mechanism, including a tool change.
    pub(crate) fn is_paused(&self) -> bool {
        matches!(self.phase(), Phase::Paused(_) | Phase::ToolChange)
    }

    /// Whether a manual tool change is pending.
    pub(crate) fn is_tool_change(&self) -> bool {
        matches!(self.phase(), Phase::ToolChange)
    }

    /// Whether the job can still make progress.
    pub(crate) fn is_active(&self) -> bool {
        matches!(
            self.phase(),
            Phase::Running | Phase::Paused(_) | Phase::ToolChange
        )
    }

    /// Whether [`JobRunner::stop`] accepts the current state.
    pub(crate) fn can_stop(&self) -> bool {
        matches!(
            self.phase(),
            Phase::Loaded | Phase::Running | Phase::Paused(_) | Phase::ToolChange
        )
    }

    /// Public job state for errors and snapshots.
    pub(crate) fn job_state(&self) -> JobState {
        match self.phase() {
            Phase::Empty | Phase::Loaded => JobState::Idle,
            Phase::Running => JobState::Running,
            Phase::Paused(_) | Phase::ToolChange => JobState::Paused,
            Phase::Completed => JobState::Completed,
            Phase::Aborted => JobState::Aborted,
            Phase::Interrupted => JobState::Interrupted,
        }
    }

    /// Copies job bookkeeping into a machine snapshot.
    ///
    /// `JobInfo` (from `ghs-core`) has no dry-run field yet, so a check run is
    /// reported through [`crate::EngineHandle::check_run`] and the
    /// `CheckStarted`/`CheckCompleted` events instead of the snapshot.
    pub(crate) fn apply_info(&self, info: &mut JobInfo) {
        if matches!(self.phase(), Phase::Empty) {
            return;
        }
        info.name = self.name.clone();
        info.state = self.job_state();
        info.line = Some(self.current_line).filter(|line| *line > 0);
        info.total_lines = self.total_lines;
        info.completed_lines = self.completed;
        info.paused = matches!(self.phase(), Phase::Paused(_) | Phase::ToolChange);
        info.interrupted = matches!(self.phase(), Phase::Interrupted);
        info.update_progress();
    }

    /// Source lines in this run.
    pub(crate) const fn total_lines(&self) -> u32 {
        self.total_lines
    }

    /// Resolved source lines.
    pub(crate) const fn completed_lines(&self) -> u32 {
        self.completed
    }

    /// Current 1-based source line.
    pub(crate) const fn current_line(&self) -> u32 {
        self.current_line
    }

    /// Index of the next source line to process.
    pub(crate) const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Source line at `index`.
    pub(crate) fn line(&self, index: usize) -> Option<&ProgramLine> {
        self.program.as_ref()?.lines.get(index)
    }

    /// Metadata for the line at `index`.
    pub(crate) fn meta(&self, index: usize) -> Option<LineMeta> {
        self.meta.get(index).copied()
    }

    /// Skips a blank, comment-only or block-deleted line.
    pub(crate) fn advance_skipped(&mut self) {
        let index = self.cursor;
        let (line, meta) = match (self.line(index), self.meta(index)) {
            (Some(line), Some(meta)) => (line.line, meta),
            _ => {
                self.cursor = self.total_lines as usize;
                return;
            }
        };
        self.cursor = index.saturating_add(1);
        self.completed = self.completed.saturating_add(1).min(self.total_lines);
        self.current_line = bounded_line(line);
        self.active_tool = meta.tool;
    }

    /// Records that the line at `index` was written to the controller.
    pub(crate) fn mark_sent(&mut self, index: usize, line: u32) {
        let meta = self.meta(index).unwrap_or(LineMeta { tool: 0, flags: 0 });
        self.in_flight = Some(InFlightLine { index, line, meta });
        self.current_line = line;
    }

    /// Acknowledges the in-flight line.
    pub(crate) fn acknowledge(&mut self, now: Instant) -> Option<AckInfo> {
        let in_flight = self.in_flight.take()?;
        self.cursor = in_flight.index.saturating_add(1);
        self.completed = self.completed.saturating_add(1).min(self.total_lines);
        self.current_line = in_flight.line;
        self.active_tool = in_flight.meta.tool;
        let elapsed = self.elapsed(now);
        let progress_due = self.take_progress_due(now);
        Some(AckInfo {
            index: in_flight.index,
            line: in_flight.line,
            meta: in_flight.meta,
            progress_due,
            elapsed,
        })
    }

    /// Marks the in-flight line as rejected and stops the job.
    pub(crate) fn fail_in_flight(&mut self, now: Instant, line: u32) -> Option<u32> {
        let in_flight = self.in_flight.take();
        if let Some(in_flight) = in_flight {
            self.current_line = in_flight.line;
            self.active_tool = in_flight.meta.tool;
        } else {
            self.current_line = line;
        }
        if matches!(self.phase(), Phase::Running) {
            self.accumulate(now);
        }
        self.phase = Some(Phase::Aborted);
        self.tool_change = None;
        Some(self.current_line)
    }

    /// Starts or restarts the loaded program from the first line.
    pub(crate) fn start(&mut self, now: Instant) -> Result<u32, EngineError> {
        if !self.has_program() {
            return Err(EngineError::NoProgram);
        }
        if !matches!(
            self.phase(),
            Phase::Loaded | Phase::Completed | Phase::Aborted | Phase::Interrupted
        ) {
            return Err(EngineError::JobState {
                action: "start",
                state: self.job_state(),
            });
        }
        self.phase = Some(Phase::Running);
        self.cursor = 0;
        self.completed = 0;
        self.current_line = 0;
        self.in_flight = None;
        self.run_started = Some(now);
        self.accumulated = Duration::ZERO;
        self.last_progress_at = None;
        self.last_progress_completed = 0;
        self.last_breakpoint_hit = None;
        self.active_tool = 0;
        self.tool_change = None;
        self.check = false;
        Ok(1)
    }

    /// Starts the loaded program as a dry run (`$C`).
    ///
    /// The runner itself does not send `$C`; the engine enters check mode
    /// first and only then calls this method. The run behaves exactly like
    /// [`JobRunner::start`] for progress, pause and stop, but it is marked as
    /// a check run until the next load or normal start.
    pub(crate) fn start_check(&mut self, now: Instant) -> Result<u32, EngineError> {
        if !self.has_program() {
            return Err(EngineError::NoProgram);
        }
        if !matches!(
            self.phase(),
            Phase::Loaded | Phase::Completed | Phase::Aborted | Phase::Interrupted
        ) {
            return Err(EngineError::JobState {
                action: "start a check run",
                state: self.job_state(),
            });
        }
        let from = self.start(now)?;
        self.check = true;
        Ok(from)
    }

    /// Whether the loaded program's current or most recent run is a check run.
    ///
    /// Set by [`JobRunner::start_check`] and cleared by a new load or a normal
    /// [`JobRunner::start`]. The flag survives completion and abort so the UI
    /// can label the result of a dry run.
    pub(crate) const fn is_check(&self) -> bool {
        self.check
    }

    /// Pauses a running job.
    pub(crate) fn pause(
        &mut self,
        now: Instant,
        reason: JobPauseReason,
    ) -> Option<(u32, JobPauseReason)> {
        if !self.is_running() {
            return None;
        }
        self.accumulate(now);
        self.phase = Some(Phase::Paused(reason));
        if reason == JobPauseReason::Breakpoint
            && let Some(line) = self.line(self.cursor)
        {
            self.current_line = bounded_line(line.line);
        }
        Some((self.current_line.max(1), reason))
    }

    /// Resumes a paused job.
    pub(crate) fn resume(&mut self, now: Instant) -> Result<(), EngineError> {
        let Phase::Paused(reason) = self.phase() else {
            return Err(EngineError::JobState {
                action: "resume",
                state: self.job_state(),
            });
        };
        if reason == JobPauseReason::Watchdog {
            return Err(EngineError::Unsupported(
                "the job was paused by the status watchdog; stop and restart it".into(),
            ));
        }
        self.phase = Some(Phase::Running);
        self.run_started = Some(now);
        Ok(())
    }

    /// Stops a loaded, running, paused or tool-change job.
    pub(crate) fn stop(&mut self, now: Instant) -> Option<u32> {
        if !self.can_stop() {
            return None;
        }
        if self.is_running() {
            self.accumulate(now);
        }
        self.phase = Some(Phase::Aborted);
        self.tool_change = None;
        Some(self.current_line)
    }

    /// Aborts an active job after an alarm, door, reset or link loss.
    pub(crate) fn abort(&mut self, now: Instant, interrupt: bool) -> Option<u32> {
        if !self.is_active() {
            return None;
        }
        if self.is_running() {
            self.accumulate(now);
        }
        self.in_flight = None;
        self.tool_change = None;
        self.phase = Some(if interrupt {
            Phase::Interrupted
        } else {
            Phase::Aborted
        });
        Some(self.current_line.max(1))
    }

    /// Marks the program complete.
    pub(crate) fn complete(&mut self, now: Instant) -> Option<Duration> {
        if !self.is_running() {
            return None;
        }
        self.accumulate(now);
        self.phase = Some(Phase::Completed);
        self.cursor = self.total_lines as usize;
        self.completed = self.total_lines;
        self.in_flight = None;
        self.tool_change = None;
        Some(self.elapsed(now))
    }

    /// Active job time, excluding paused periods.
    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        let running = self.run_started.map_or(Duration::ZERO, |started| {
            now.saturating_duration_since(started)
        });
        self.accumulated.saturating_add(running)
    }

    /// Emits a breakpoint pause once per line.
    pub(crate) fn breakpoint_hit(&mut self, line: u32) -> bool {
        if !self.breakpoints.contains(&line) {
            return false;
        }
        if self.last_breakpoint_hit == Some(line) {
            self.last_breakpoint_hit = None;
            return false;
        }
        self.last_breakpoint_hit = Some(line);
        true
    }

    /// Enables or disables a breakpoint.
    pub(crate) fn set_breakpoint(&mut self, line: u32, enabled: bool) -> Result<(), EngineError> {
        if self.program.is_none() {
            return Err(EngineError::NoProgram);
        }
        if line == 0 || line > self.total_lines {
            return Err(EngineError::InvalidCommand(format!(
                "breakpoint line {line} is outside the program"
            )));
        }
        if enabled {
            self.breakpoints.insert(line);
            if self.last_breakpoint_hit == Some(line) {
                self.last_breakpoint_hit = None;
            }
        } else {
            self.breakpoints.remove(&line);
        }
        Ok(())
    }

    /// Enables or disables the virtual optional-stop flag.
    pub(crate) fn set_optional_stop(&mut self, enabled: bool) {
        self.optional_stop = enabled;
    }

    /// Current optional-stop flag.
    pub(crate) const fn optional_stop(&self) -> bool {
        self.optional_stop
    }

    /// Enables or disables the virtual single-block flag.
    pub(crate) fn set_single_block(&mut self, enabled: bool) {
        self.single_block = enabled;
    }

    /// Current single-block flag.
    pub(crate) const fn single_block(&self) -> bool {
        self.single_block
    }

    /// Enables or disables block delete.
    pub(crate) fn set_block_delete(&mut self, enabled: bool) {
        self.block_delete = enabled;
    }

    /// Current block-delete flag.
    pub(crate) const fn block_delete(&self) -> bool {
        self.block_delete
    }

    /// Clears the virtual optional-stop, single-block and block-delete flags.
    pub(crate) fn reset_toggles(&mut self) {
        self.optional_stop = false;
        self.single_block = false;
        self.block_delete = false;
    }

    /// Begins the tool-change handshake after an acknowledged `M6`.
    pub(crate) fn begin_tool_change(
        &mut self,
        now: Instant,
        tool: u32,
        line: u32,
    ) -> Option<PendingToolChange> {
        if self.tool_change.is_some() || !matches!(self.phase(), Phase::Running | Phase::Paused(_))
        {
            return None;
        }
        if self.is_running() {
            self.accumulate(now);
        }
        let pending = PendingToolChange {
            tool,
            line: line.max(1),
            acknowledged: false,
            saw_tool_state: false,
            deadline: now.checked_add(TOOL_CHANGE_TIMEOUT).unwrap_or(now),
        };
        self.tool_change = Some(pending);
        self.phase = Some(Phase::ToolChange);
        Some(pending)
    }

    /// Records that the controller reported the `Tool` state.
    pub(crate) fn note_tool_state(&mut self) {
        if let Some(pending) = self.tool_change.as_mut() {
            pending.saw_tool_state = true;
        }
    }

    /// Returns the requested tool and line for an in-flight `M6` block.
    pub(crate) fn in_flight_tool_change(&self) -> Option<(u32, u32)> {
        let in_flight = self.in_flight?;
        if in_flight.meta.is_tool_change() {
            Some((in_flight.meta.tool, in_flight.line))
        } else {
            None
        }
    }

    /// Whether the in-flight block will pause the job when it is acknowledged.
    pub(crate) fn in_flight_pauses_job(&self) -> bool {
        let Some(in_flight) = self.in_flight else {
            return false;
        };
        in_flight.meta.is_program_stop()
            || in_flight.meta.is_tool_change()
            || (in_flight.meta.is_optional_stop() && self.optional_stop)
    }

    /// Whether the in-flight block is `M2` or `M30`.
    ///
    /// A check run uses this to accept a controller that returns to `Idle` at
    /// program end while the final block is still unacknowledged.
    pub(crate) fn in_flight_is_program_end(&self) -> bool {
        self.in_flight
            .is_some_and(|in_flight| in_flight.meta.is_program_end())
    }

    /// Marks the pending tool change acknowledged with `0xA3`.
    pub(crate) fn acknowledge_tool_change(&mut self) -> bool {
        let Some(pending) = self.tool_change.as_mut() else {
            return false;
        };
        if pending.acknowledged {
            return true;
        }
        pending.acknowledged = true;
        true
    }

    /// Whether the pending tool change was acknowledged.
    pub(crate) const fn tool_change_acknowledged(&self) -> bool {
        match self.tool_change {
            Some(pending) => pending.acknowledged,
            None => false,
        }
    }

    /// Returns `(line, tool)` when the controller never entered `Tool`.
    pub(crate) fn tool_change_stalled(&self, now: Instant) -> Option<(u32, u32)> {
        let pending = self.tool_change?;
        if !pending.saw_tool_state && now >= pending.deadline {
            return Some((pending.line, pending.tool));
        }
        None
    }

    /// Continues a tool-change job after cycle start.
    pub(crate) fn finish_tool_change(&mut self, now: Instant) -> Result<(), EngineError> {
        if !self.is_tool_change() {
            return Err(EngineError::JobState {
                action: "continue after a tool change",
                state: self.job_state(),
            });
        }
        if !self.tool_change_acknowledged() {
            return Err(EngineError::JobState {
                action: "continue after a tool change",
                state: self.job_state(),
            });
        }
        self.tool_change = None;
        self.phase = Some(Phase::Running);
        self.run_started = Some(now);
        Ok(())
    }

    /// Modal tool number after the last resolved line.
    pub(crate) const fn active_tool(&self) -> u32 {
        self.active_tool
    }

    fn accumulate(&mut self, now: Instant) {
        if let Some(started) = self.run_started.take() {
            self.accumulated = self
                .accumulated
                .saturating_add(now.saturating_duration_since(started));
        }
    }

    fn take_progress_due(&mut self, now: Instant) -> bool {
        let interval_due = self
            .last_progress_at
            .is_none_or(|last| now.saturating_duration_since(last) >= PROGRESS_INTERVAL);
        let step_due =
            self.completed.saturating_sub(self.last_progress_completed) >= PROGRESS_LINE_STEP;
        if interval_due || step_due {
            self.last_progress_at = Some(now);
            self.last_progress_completed = self.completed;
            true
        } else {
            false
        }
    }
}

fn code_is(value: f64, code: f64) -> bool {
    (value - code).abs() < 1.0e-9
}

fn bounded_line(line: usize) -> u32 {
    u32::try_from(line).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(runner: &mut JobRunner, source: &str) -> Arc<Program> {
        runner
            .load("test.nc".into(), source)
            .expect("valid program")
    }

    #[test]
    fn load_counts_lines_and_metadata() {
        let mut runner = JobRunner::new();
        let program = load(
            &mut runner,
            "G21\nG0 X1\nT3 M6\nM0\nM1\nG1 X2 F100\nM2\nG0 X9\n",
        );
        assert_eq!(program.lines.len(), 8);
        assert_eq!(runner.total_lines(), 7, "the line after M2 is not streamed");
        assert_eq!(runner.meta(1).expect("meta").tool, 0);
        assert_eq!(runner.meta(2).expect("meta").tool, 3);
        assert!(runner.meta(2).expect("meta").is_tool_change());
        assert!(runner.meta(3).expect("meta").is_program_stop());
        assert!(runner.meta(4).expect("meta").is_optional_stop());
        assert!(runner.meta(6).expect("meta").is_program_end());
    }

    #[test]
    fn invalid_program_is_rejected() {
        let mut runner = JobRunner::new();
        let error = runner
            .load("bad.nc".into(), "G1 X\n")
            .expect_err("invalid program");
        assert!(matches!(error, EngineError::InvalidProgram(_)));
        assert!(!runner.has_program());
    }

    #[test]
    fn start_resets_cursor_and_completion() {
        let mut runner = JobRunner::new();
        load(&mut runner, "G0 X1\nM2\n");
        let now = Instant::now();
        assert_eq!(runner.start(now).expect("start"), 1);
        assert_eq!(runner.current_line(), 0);
        assert!(runner.is_running());
        let meta = runner.meta(0).expect("meta");
        runner.mark_sent(0, 1);
        let ack = runner.acknowledge(now).expect("ack");
        assert_eq!(ack.line, 1);
        assert_eq!(ack.meta, meta);
        assert_eq!(runner.completed_lines(), 1);
        assert_eq!(runner.cursor(), 1);
    }

    #[test]
    fn check_run_is_flagged_and_cleared_by_a_normal_start() {
        let mut runner = JobRunner::new();
        load(&mut runner, "G0 X1\nM2\n");
        assert!(!runner.is_check());
        runner.start_check(Instant::now()).expect("check start");
        assert!(runner.is_check());
        assert!(runner.is_running());
        runner.complete(Instant::now()).expect("complete");
        assert!(runner.is_check(), "a finished dry run stays labelled");

        runner.start(Instant::now()).expect("normal restart");
        assert!(!runner.is_check());

        runner.complete(Instant::now()).expect("complete");
        load(&mut runner, "G0 X2\nM2\n");
        assert!(!runner.is_check(), "a new program is not a dry run");
    }

    #[test]
    fn breakpoint_pauses_once_per_line() {
        let mut runner = JobRunner::new();
        load(&mut runner, "G0 X1\nG0 X2\nM2\n");
        runner.set_breakpoint(2, true).expect("breakpoint");
        assert!(runner.breakpoint_hit(2));
        assert!(!runner.breakpoint_hit(2));
    }

    #[test]
    fn toggles_default_off_and_reset_together() {
        let mut runner = JobRunner::new();
        assert!(!runner.optional_stop());
        assert!(!runner.single_block());
        assert!(!runner.block_delete());

        runner.set_optional_stop(true);
        assert!(runner.optional_stop());
        assert!(!runner.single_block(), "optional stop is independent");
        assert!(!runner.block_delete(), "optional stop is independent");

        runner.set_single_block(true);
        assert!(runner.single_block());
        assert!(!runner.block_delete(), "single block is independent");

        runner.set_block_delete(true);
        assert!(runner.block_delete());

        runner.reset_toggles();
        assert!(!runner.optional_stop());
        assert!(!runner.single_block());
        assert!(!runner.block_delete());
    }

    #[test]
    fn tool_change_timeout_only_before_the_tool_state() {
        let mut runner = JobRunner::new();
        load(&mut runner, "T4 M6\nM2\n");
        let now = Instant::now();
        runner.start(now).expect("start");
        let pending = runner.begin_tool_change(now, 4, 1).expect("pending");
        assert_eq!(pending.tool, 4);
        assert!(
            runner
                .tool_change_stalled(now + TOOL_CHANGE_TIMEOUT)
                .is_some()
        );
        runner.note_tool_state();
        assert!(
            runner
                .tool_change_stalled(now + TOOL_CHANGE_TIMEOUT)
                .is_none()
        );
    }
}
