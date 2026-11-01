//! The stateful parser and its display-toolpath emitter.
//!
//! # Coordinates
//!
//! Program words are interpreted in the active [`Units`](ghs_core::Units) and
//! converted to millimetres. The emitted [`Toolpath`](ghs_viewer::Toolpath) is
//! in **machine/display millimetres**:
//!
//! 1. `G51` scaling and caller-supplied rotation are applied about their pivots.
//! 2. The active work offset (`G54`..`G59.3`), `G92`, and `G43.1` tool-length
//!    offset are added.
//!
//! With the default configuration (all offsets zero) the toolpath is identical
//! to the program coordinates. `G53` moves and `G28`/`G30` home moves target
//! machine coordinates directly.
//!
//! # Not applied to the display path
//!
//! - Stored `G28`/`G30` positions unless supplied in [`ParserConfig`].
//! - Cutter radius compensation (grblHAL does not support it; `G41`/`G42`
//!   are rejected).
//! - Acceleration, rapid rate, and `G93` inverse-time timing.
//! - Firmware-specific transforms such as skew compensation and backlash.
//! - `G4`/`G82`/`G86`/`G89` dwell time (no geometry).
//! - Expressions, parameters, and flow control (rejected; VIEW-02 follow-up).

use ghs_core::Position;
use ghs_viewer::{Point3, SegmentKind, Toolpath};

use crate::block::Block;
use crate::config::ParserConfig;
use crate::error::{ParseError, ParseErrorKind, ParseWarning, WarningKind};
use crate::lexer::{LexedLine, lex_line};
use crate::planner::{Frame, Plan, PlannedMove, Planner};
use crate::program::{Program, ProgramLine, ProgramStats};
use crate::state::ParserState;

/// G-code parser with accumulating program, toolpath, and statistics.
///
/// Use [`Parser::parse_line`] for MDI-style incremental input, or
/// [`parse_program`] for a whole file. A failed block leaves the modal state
/// untouched, matching the controller's atomic block execution.
#[derive(Debug, Clone)]
pub struct Parser {
    config: ParserConfig,
    state: ParserState,
    offsets: [Position; 9],
    homes: [Option<[f64; 3]>; 2],
    machine: [f64; 3],
    toolpath: Toolpath,
    stats: ProgramStats,
    lines: Vec<ProgramLine>,
    warnings: Vec<ParseWarning>,
    next_line: usize,
    ended: bool,
    ended_warning_emitted: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Creates a parser with the default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Creates a parser with the given configuration.
    ///
    /// The parser starts at program position `(0, 0, 0)`, so the initial
    /// machine position is the active work offset.
    #[must_use]
    pub fn with_config(config: ParserConfig) -> Self {
        let config = config.normalized();
        let state = ParserState::default();
        let offsets = config.offsets;
        let homes = [config.home_position, config.second_home_position];
        let machine = Frame::new(&state, &offsets).to_machine(state.position);
        Self {
            config,
            state,
            offsets,
            homes,
            machine,
            toolpath: Toolpath::new(),
            stats: ProgramStats::default(),
            lines: Vec::new(),
            warnings: Vec::new(),
            next_line: 1,
            ended: false,
            ended_warning_emitted: false,
        }
    }

    /// Returns the configuration in effect.
    #[must_use]
    pub fn config(&self) -> &ParserConfig {
        &self.config
    }

    /// Returns the current modal state.
    #[must_use]
    pub fn state(&self) -> &ParserState {
        &self.state
    }

    /// Replaces the modal state, for example from a controller's `$G` report.
    ///
    /// The current machine position is kept; the program position is derived
    /// from it through the new state's offsets and transforms.
    pub fn set_state(&mut self, state: ParserState) {
        self.state = state;
        self.machine = Frame::new(&self.state, &self.offsets).to_machine(self.state.position);
    }

    /// Returns the current work-offset table in `G54`..`G59.3` order.
    #[must_use]
    pub fn offsets(&self) -> &[Position; 9] {
        &self.offsets
    }

    /// Replaces the work-offset table and re-derives the program position.
    pub fn set_offsets(&mut self, offsets: [Position; 9]) {
        self.offsets = offsets;
        self.state.position = Frame::new(&self.state, &self.offsets).to_work(self.machine);
    }

    /// Returns the stored `G28`/`G30` positions in machine coordinates.
    #[must_use]
    pub fn homes(&self) -> &[Option<[f64; 3]>; 2] {
        &self.homes
    }

    /// Returns the current machine/display position in millimetres.
    #[must_use]
    pub fn machine_position(&self) -> [f64; 3] {
        self.machine
    }

    /// Moves the tracked machine position without emitting geometry.
    ///
    /// Intended for start-from-line and after a controller reconnect.
    pub fn set_machine_position(&mut self, machine: [f64; 3]) {
        self.machine = machine;
        self.state.position = Frame::new(&self.state, &self.offsets).to_work(machine);
    }

    /// Returns the toolpath generated so far.
    #[must_use]
    pub fn toolpath(&self) -> &Toolpath {
        &self.toolpath
    }

    /// Returns the warnings collected so far.
    #[must_use]
    pub fn warnings(&self) -> &[ParseWarning] {
        &self.warnings
    }

    /// Parses one line, using the parser's next source line number.
    ///
    /// # Errors
    ///
    /// Returns every diagnostic for the line. The block is not applied.
    pub fn parse_line(&mut self, source: &str) -> Result<ProgramLine, Vec<ParseError>> {
        let line = self.next_line;
        self.next_line = self.next_line.saturating_add(1);
        self.parse_line_at(line, source)
    }

    /// Parses one line with an explicit 1-based source line number.
    ///
    /// The line number is used in diagnostics and stored in the returned
    /// [`ProgramLine`].
    ///
    /// # Errors
    ///
    /// Returns every diagnostic for the line. The block is not applied.
    pub fn parse_line_at(
        &mut self,
        line: usize,
        source: &str,
    ) -> Result<ProgramLine, Vec<ParseError>> {
        if line >= self.next_line {
            self.next_line = line.saturating_add(1);
        }
        let source = source.strip_suffix('\r').unwrap_or(source);
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        if source.len() > self.config.max_line_bytes {
            return Err(vec![ParseError::new(
                line,
                1,
                ParseErrorKind::LineTooLong,
                format!(
                    "line is {} bytes; the limit is {}",
                    source.len(),
                    self.config.max_line_bytes
                ),
            )]);
        }
        let lexed = lex_line(line, source, self.config.max_words)?;
        self.parse_lexed(line, source, lexed)
    }

    /// Applies an already-lexed line.
    ///
    /// This is the extension point for the deferred expression and flow-control
    /// pass (VIEW-02): a preprocessor can rewrite the word list of a
    /// [`LexedLine`] and then feed it here. The method itself has no knowledge
    /// of parameters or expressions.
    ///
    /// # Errors
    ///
    /// Returns every diagnostic for the line. The block is not applied.
    pub fn parse_lexed(
        &mut self,
        line: usize,
        source: &str,
        lexed: LexedLine,
    ) -> Result<ProgramLine, Vec<ParseError>> {
        if line >= self.next_line {
            self.next_line = line.saturating_add(1);
        }
        if lexed.words.len() > self.config.max_words {
            return Err(vec![ParseError::new(
                line,
                1,
                ParseErrorKind::TooManyWords,
                format!(
                    "{} words; the limit is {}",
                    lexed.words.len(),
                    self.config.max_words
                ),
            )]);
        }

        let block = Block::parse(line, &lexed)?;
        let number = block.number;
        let planner = Planner {
            state: self.state,
            offsets: self.offsets,
            homes: self.homes,
            machine: self.machine,
            config: &self.config,
            line,
        };
        let plan = planner.plan(&block)?;

        let mut program_line = ProgramLine {
            line,
            number,
            source: source.to_string(),
            words: lexed.words,
            comments: lexed.comments,
            block_delete: lexed.block_delete,
            checksum: lexed.checksum,
            segments: 0..0,
        };

        if self.ended && !plan.moves.is_empty() && !self.ended_warning_emitted {
            self.ended_warning_emitted = true;
            self.warnings.push(ParseWarning::new(
                line,
                WarningKind::MotionAfterProgramEnd,
                warning_message(WarningKind::MotionAfterProgramEnd),
            ));
        }
        for kind in &plan.warnings {
            self.warnings
                .push(ParseWarning::new(line, *kind, warning_message(*kind)));
        }

        let start = self.toolpath.len();
        self.execute(plan);
        program_line.segments = start..self.toolpath.len();

        if program_line.has_words() {
            self.stats.blocks += 1;
        }
        self.stats.words += program_line.words.len();
        self.lines.push(program_line.clone());
        Ok(program_line)
    }

    /// Finishes the program and returns its lines, geometry, and statistics.
    #[must_use]
    pub fn finish(mut self) -> Program {
        self.stats.lines = self.lines.len();
        self.stats.segments = self.toolpath.len();
        let bounds = self.toolpath.bounds();
        Program {
            lines: self.lines,
            toolpath: self.toolpath,
            bounds,
            stats: self.stats,
            state: self.state,
            offsets: self.offsets,
            warnings: self.warnings,
        }
    }

    /// Applies a validated plan and emits its geometry.
    fn execute(&mut self, plan: Plan) {
        let Plan {
            state,
            offsets,
            homes,
            moves,
            warnings: _,
            arc_moves,
            canned_cycles,
            tool_changes,
            program_end,
            feed_observed,
            spindle_observed,
            selected_tool,
        } = plan;
        let frame = Frame::new(&state, &offsets);
        let mut machine = self.machine;
        let mut work = frame.to_work(machine);
        let tool = u16::try_from(state.tool).unwrap_or(u16::MAX);

        for planned in &moves {
            match *planned {
                PlannedMove::Line { to_work, kind } => {
                    let to_machine = frame.to_machine(to_work);
                    self.emit_line(machine, to_machine, kind, tool);
                    machine = to_machine;
                    work = to_work;
                }
                PlannedMove::MachineLine { to_machine, kind } => {
                    self.emit_line(machine, to_machine, kind, tool);
                    machine = to_machine;
                    work = frame.to_work(to_machine);
                }
                PlannedMove::Arc {
                    to_work,
                    center_work,
                    plane,
                    clockwise,
                } => {
                    let end_machine = frame.to_machine(to_work);
                    let center_machine = frame.to_machine(center_work);
                    self.emit_arc(machine, end_machine, center_machine, plane, clockwise, tool);
                    machine = end_machine;
                    work = to_work;
                }
            }
        }

        self.machine = machine;
        self.state = state;
        self.state.position = work;
        self.offsets = offsets;
        self.homes = homes;
        self.stats.arc_moves += arc_moves;
        self.stats.canned_cycles += canned_cycles;
        self.stats.tool_changes += tool_changes;
        if let Some(feed) = feed_observed {
            self.stats.observe_feed(feed);
        }
        if let Some(rpm) = spindle_observed {
            self.stats.max_spindle_rpm = self.stats.max_spindle_rpm.max(rpm);
        }
        if let Some(tool) = selected_tool {
            self.stats.observe_tool(tool);
        }
        if program_end {
            self.ended = true;
        }
    }

    /// Emits one straight segment and accounts for it.
    fn emit_line(
        &mut self,
        from_machine: [f64; 3],
        to_machine: [f64; 3],
        kind: SegmentKind,
        tool: u16,
    ) {
        if distance(from_machine, to_machine) <= 1.0e-9 {
            return;
        }
        let kind = if kind == SegmentKind::Feed
            && (from_machine[0] - to_machine[0]).abs() <= 1.0e-9
            && (from_machine[1] - to_machine[1]).abs() <= 1.0e-9
            && to_machine[2] < from_machine[2]
        {
            SegmentKind::Plunge
        } else {
            kind
        };
        let from = point(from_machine);
        let to = point(to_machine);
        let length = f64::from(from.distance(to));
        self.toolpath.append_line(from, to, kind, 0, tool);
        self.account_segment(kind, length, tool);
    }

    /// Emits an arc as chords, splitting at cardinal angles.
    ///
    /// The viewer's [`Arc`] helpers place tessellated points relative to the
    /// plane origin instead of the arc center, so this layer generates the
    /// chords itself. Including every cardinal angle as a vertex means the
    /// viewer's endpoint-based bounds still contain the exact circle extents.
    /// Helical arcs carry an interpolated normal coordinate and therefore need
    /// no planar slicing.
    fn emit_arc(
        &mut self,
        start: [f64; 3],
        end: [f64; 3],
        center: [f64; 3],
        plane: crate::state::Plane,
        clockwise: bool,
        tool: u16,
    ) {
        let (u, v) = plane.axes();
        let normal = plane.normal_axis();
        let center_uv = [center[u], center[v]];
        let start_uv = [start[u], start[v]];
        let end_uv = [end[u], end[v]];
        let radius = uv_distance(start_uv, center_uv);
        let start_angle = (start_uv[1] - center_uv[1]).atan2(start_uv[0] - center_uv[0]);
        let end_angle = (end_uv[1] - center_uv[1]).atan2(end_uv[0] - center_uv[0]);
        if !radius.is_finite() || radius <= 0.0 {
            tracing::warn!("arc has no usable radius; drawing a straight move");
            self.emit_line(start, end, SegmentKind::Feed, tool);
            return;
        }
        let sweep = directed_sweep(start_angle, end_angle, clockwise);
        let total = sweep.abs();
        let tolerance = self.config.chord_tolerance_mm.max(1.0e-6);
        let ratio = (tolerance / radius).clamp(0.0, 1.0);
        let step = (2.0 * (1.0 - ratio).acos()).clamp(1.0e-9, std::f64::consts::FRAC_PI_3);

        // Split at the plane's cardinal angles so the extrema are vertices.
        let mut fractions = vec![0.0_f64, 1.0_f64];
        for cardinal in [
            0.0,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
            std::f64::consts::PI * 1.5,
        ] {
            let traveled = if clockwise {
                start_angle - cardinal
            } else {
                cardinal - start_angle
            }
            .rem_euclid(std::f64::consts::TAU);
            if traveled > 1.0e-9 && traveled < total - 1.0e-9 {
                fractions.push(traveled / total);
            }
        }
        fractions.sort_by(f64::total_cmp);
        fractions.dedup_by(|first, second| (*first - *second).abs() <= 1.0e-12);

        let normal_start = start[normal];
        let normal_end = end[normal];
        let mut previous = start;
        let mut count = 0usize;
        for interval in fractions.windows(2) {
            let (t0, t1) = (interval[0], interval[1]);
            let sub_sweep = total * (t1 - t0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let chords = (sub_sweep / step).ceil().clamp(1.0, 65_536.0) as usize;
            for chord in 1..=chords {
                #[allow(clippy::cast_precision_loss)]
                let local = chord as f64 / chords as f64;
                let t = t0 + (t1 - t0) * local;
                let angle = start_angle + sweep * t;
                let uv = [
                    center_uv[0] + radius * angle.cos(),
                    center_uv[1] + radius * angle.sin(),
                ];
                let mut next = [0.0; 3];
                next[u] = uv[0];
                next[v] = uv[1];
                next[normal] = normal_start + (normal_end - normal_start) * t;
                if t1 >= 1.0 - 1.0e-12 {
                    next = end;
                }
                if distance(previous, next) <= 1.0e-12 {
                    continue;
                }
                self.toolpath
                    .append_line(point(previous), point(next), SegmentKind::Arc, 0, tool);
                previous = next;
                count += 1;
            }
        }
        if distance(previous, end) > 1.0e-9 {
            self.toolpath
                .append_line(point(previous), point(end), SegmentKind::Arc, 0, tool);
            count += 1;
        }
        let planar_length = radius * total;
        let length = planar_length.hypot(normal_end - normal_start);
        self.account_segments(SegmentKind::Arc, count, length, tool);
    }

    /// Accounts for one segment's kind and length.
    fn account_segment(&mut self, kind: SegmentKind, length_mm: f64, tool: u16) {
        self.account_segments(kind, 1, length_mm, tool);
    }

    /// Accounts for `count` segments of `kind` with a total exact length.
    fn account_segments(&mut self, kind: SegmentKind, count: usize, length_mm: f64, tool: u16) {
        if count == 0 {
            return;
        }
        match kind {
            SegmentKind::Rapid => {
                self.stats.rapid_segments += count;
                self.stats.rapid_length_mm += length_mm;
            }
            SegmentKind::Feed => {
                self.stats.feed_segments += count;
                self.stats.cutting_length_mm += length_mm;
            }
            SegmentKind::Plunge => {
                self.stats.plunge_segments += count;
                self.stats.cutting_length_mm += length_mm;
            }
            SegmentKind::Arc => {
                self.stats.arc_segments += count;
                self.stats.cutting_length_mm += length_mm;
            }
            SegmentKind::Probe => {
                self.stats.probe_segments += count;
            }
        }
        self.stats.path_length_mm += length_mm;
        self.stats.observe_tool(tool);
    }
}

/// Parses a whole program with the default configuration.
///
/// # Errors
///
/// Returns every collected [`ParseError`], sorted by line and column, when any
/// line fails. A program with errors produces no [`Program`].
pub fn parse_program(text: &str) -> Result<Program, Vec<ParseError>> {
    parse_program_with(text, &ParserConfig::default())
}

/// Parses a whole program with explicit configuration.
///
/// # Errors
///
/// Returns every collected [`ParseError`], sorted by line and column, when any
/// line fails. Parsing stops after
/// [`ParserConfig::max_errors`] diagnostics.
pub fn parse_program_with(text: &str, config: &ParserConfig) -> Result<Program, Vec<ParseError>> {
    let mut parser = Parser::with_config(config.clone());
    let max_errors = parser.config.max_errors;
    let mut errors = Vec::new();
    for (index, source) in SourceLines::new(text).enumerate() {
        let line = index.saturating_add(1);
        if let Err(mut line_errors) = parser.parse_line_at(line, source) {
            errors.append(&mut line_errors);
            if errors.len() >= max_errors {
                errors.push(ParseError::new(
                    line,
                    1,
                    ParseErrorKind::TooManyErrors,
                    format!("stopped after {max_errors} errors"),
                ));
                break;
            }
        }
    }
    errors.sort_by_key(|error| (error.line, error.column));
    if errors.is_empty() {
        Ok(parser.finish())
    } else {
        Err(errors)
    }
}

/// Human-readable text for a warning class.
fn warning_message(kind: WarningKind) -> &'static str {
    match kind {
        WarningKind::AssumedHomePosition => {
            "G28/G30 has no stored position; the machine origin was assumed"
        }
        WarningKind::MotionAfterProgramEnd => {
            "motion after M2/M30 will not execute on the controller"
        }
        WarningKind::EmptyCannedCycle => {
            "canned-cycle depth is above the R plane; no material is removed"
        }
    }
}

/// Converts f64 machine coordinates into the viewer's f32 point.
fn point(value: [f64; 3]) -> Point3 {
    Point3::new(value[0] as f32, value[1] as f32, value[2] as f32)
}

/// Distance between two f64 points.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Distance between two f64 points in a plane.
fn uv_distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

/// Directed arc sweep from `start` to `end`, treating a zero sweep as a full
/// circle (the G-code full-circle convention).
fn directed_sweep(start: f64, end: f64, clockwise: bool) -> f64 {
    let mut delta = if clockwise { start - end } else { end - start };
    delta = delta.rem_euclid(std::f64::consts::TAU);
    if !delta.is_finite() {
        return 0.0;
    }
    if delta <= 1.0e-9 {
        delta = std::f64::consts::TAU;
    }
    if clockwise { -delta } else { delta }
}

/// Iterator over source lines, accepting `\n`, `\r\n`, and `\r`.
struct SourceLines<'a> {
    rest: &'a str,
    done: bool,
}

impl<'a> SourceLines<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            done: text.is_empty(),
        }
    }
}

impl<'a> Iterator for SourceLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let bytes = self.rest.as_bytes();
        let separator = bytes
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r');
        match separator {
            Some(index) => {
                let line = &self.rest[..index];
                let mut next = index + 1;
                let paired = matches!(
                    (bytes[index], bytes.get(index + 1)),
                    (b'\r', Some(b'\n')) | (b'\n', Some(b'\r'))
                );
                if paired {
                    next += 1;
                }
                self.rest = &self.rest[next..];
                if self.rest.is_empty() {
                    self.done = true;
                }
                Some(line)
            }
            None => {
                self.done = true;
                Some(self.rest)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_lines_accept_all_terminators() {
        let lines: Vec<&str> = SourceLines::new("a\nb\r\nc\rd").collect();
        assert_eq!(lines, vec!["a", "b", "c", "d"]);
        assert!(SourceLines::new("").next().is_none());
        let one: Vec<&str> = SourceLines::new("only").collect();
        assert_eq!(one, vec!["only"]);
        let mixed: Vec<&str> = SourceLines::new("a\n\rb").collect();
        assert_eq!(mixed, vec!["a", "b"]);
    }

    #[test]
    fn point_conversion_is_straightforward() {
        assert_eq!(point([1.0, 2.0, -3.0]), Point3::new(1.0, 2.0, -3.0));
    }
}
