//! Sender-side height-map compensation for a whole program.
//!
//! This is the pure G-code rewrite behind an explicit **Apply height map**
//! action. It is not a live stream correction: the caller parses the source
//! once, rewrites it, and loads the result for preview or a job. The original
//! program is never mutated, so the app can keep it for Revert.
//!
//! # Algorithm
//!
//! `G0`/`G1` moves are split so every emitted XY sub-move is at most
//! [`HeightCompensationOptions::max_step_mm`] long. `G2`/`G3` arcs are split at
//! an angle that keeps each chord within the same limit. At every sub-move
//! endpoint the callback is sampled in **machine** coordinates
//! (`work XY + work_offset_xy_mm`) and its millimetre delta is added to Z.
//! Line moves interpolate the original Z linearly between the endpoints, so a
//! sloped move keeps its slope plus the local compensation. The program's
//! initial position is not sampled: compensation begins at the first emitted
//! endpoint, which is the controller's current point.
//!
//! A pure Z move is a single sub-move and **is** compensated at its XY point,
//! matching ioSender. A block that does not move (an `F`-only line, a
//! non-moving `G0`/`G1`, or a comment/modal-only line) is copied and never
//! sampled. That is the ioSender issue #451 regression class.
//!
//! # Preserved and rejected constructs
//!
//! Comments, blank lines, `M3`/`M5`, `M7`/`M9`, `S`, `F`, `G4`, tool changes,
//! `G90`/`G91`, `G20`/`G21`, `G17`-`G19`, `G43.1`/`G49` (Z frame only), and
//! other modal state lines are kept. The output stays in the program's own
//! unit mode; it is never silently converted to millimetres.
//!
//! On a split motion line, `F`, `S`, `T`, spindle start, and coolant on stay
//! with the first sub-move; spindle/coolant stop, tool change, and program flow
//! stay with the last sub-move. A program end can therefore never truncate a
//! split move, and a spindle starts before the cut.
//!
//! The following are rejected with
//! [`HeightCompensationError::UnsupportedConstruct`] instead of emitting
//! geometry that would not match the machine:
//!
//! - `G53` (machine-coordinate motion) and `G28`/`G30` (home motion).
//! - Probing moves and canned cycles.
//! - `G10`, `G92`/`G92.1`-`G92.3`, and `G51` scaling: the caller supplies one
//!   fixed work offset, so a frame change mid-program cannot be represented.
//!   A `G54`-`G59.3` selection is accepted while it stays on one coordinate
//!   system; switching to a different one is rejected.
//! - `G18`/`G19` arcs. Straight moves in those planes are fine.
//! - Helical input arcs (a Z change across a `G17` arc). The transformed
//!   sub-arcs themselves may have a small Z change between endpoints because
//!   the compensation varies; that generated helical motion is intentional and
//!   supported by grblHAL.
//! - `G93` inverse-time feed, whose value must be recomputed per sub-move.
//! - Lines carrying a `*` checksum, and `N` words (see below).
//!
//! Absolute arc centers (`G90.1`/`G91.1`) are rejected by the parser before
//! this module runs, with [`ParseErrorKind::UnsupportedCode`](crate::ParseErrorKind::UnsupportedCode).
//!
//! # N line numbers
//!
//! Splitting one source block into several output blocks would duplicate its
//! `N` word. Rather than renumber a program (which would change references an
//! operator or post-processor may rely on), all `N` words are removed and a
//! single [`HeightCompensationWarningKind::LineNumbersDropped`] warning is
//! returned. The controller does not need `N` words to run the program.
//!
//! # Outside the map
//!
//! `None` from the callback means the XY point is outside the mapped area or
//! its cell is invalid. [`OutsideMap::Leave`] keeps the programmed Z for that
//! sub-move and counts it as uncompensated; [`OutsideMap::Reject`] fails the
//! whole transform with [`HeightCompensationError::OutsideMap`]. A missing
//! cell is never extrapolated.
//!
//! # Coordinates and precision
//!
//! Coordinates are emitted in the active `G20`/`G21` units with up to six
//! decimal places in those units, trailing zeros trimmed (a non-zero value
//! that would round to zero keeps more digits). `I`/`J` center offsets are
//! always incremental from the sub-arc start, as grblHAL defines them. `R`
//! arcs are resolved to a center and re-emitted as `I`/`J`, so splitting never
//! collapses the arc into a chord. Output lines use `\n` endings.
//!
//! # Limitations
//!
//! Only an XY work translation is modelled. Controller-side WCS rotation,
//! skew, backlash, and machine tilt are outside this API; feed such effects
//! into the map or compensate in a later layer. Source-level `G51`, `G10`, and
//! `G92` frame changes are rejected rather than guessed. A tool change that
//! changes the Z datum is the caller's responsibility: the height map belongs
//! to one setup and must be re-probed after a WCS or tool change.

use std::f64::consts::TAU;

use ghs_core::{Units, Wcs};
use thiserror::Error;

use crate::block::{Block, GCode, GGroup, MCode};
use crate::config::MAX_COORDINATE_MM;
use crate::error::ParseError;
use crate::lexer::LexedLine;
use crate::parser::{Parser, parse_program};
use crate::planner::{Plan, PlannedMove, Planner};
use crate::program::ProgramLine;
use crate::state::{DistanceMode, MotionMode, ParserState, Plane};

/// Smallest accepted XY subdivision step, in millimetres.
///
/// A smaller value would generate an unreasonable number of output blocks for
/// ordinary moves without improving a real height map.
pub const MIN_HEIGHT_MAP_STEP_MM: f64 = 1.0e-3;

/// Largest accepted XY subdivision step, in millimetres.
///
/// This matches the largest height-map grid spacing accepted by `ghs-probe`;
/// a larger value only means fewer sub-moves.
pub const MAX_HEIGHT_MAP_STEP_MM: f64 = 1_000.0;

/// Safety cap on generated output lines.
///
/// A compact grid step over a long program must fail rather than allocate an
/// unbounded program. The cap matches the probe crate's compensated-segment
/// budget.
pub const MAX_COMPENSATED_OUTPUT_LINES: usize = 1_000_000;

/// Two points closer than this are the same point for motion detection.
const EPSILON_MM: f64 = 1.0e-9;

/// What to do when the height callback has no value for a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutsideMap {
    /// Keep the programmed Z for that endpoint and count the sub-move as
    /// uncompensated.
    Leave,
    /// Fail the whole transform with [`HeightCompensationError::OutsideMap`].
    Reject,
}

/// Inputs for [`apply_height_compensation`].
///
/// The callback receives machine XY: `machine = work + work_offset_xy_mm`.
/// `work_offset_xy_mm` must be the XY work offset of the coordinate system the
/// program runs in (for example the active `G54` offset when the program does
/// not change WCS). The first explicit `G54`-`G59.3` word is assumed to name
/// that same system; a later word naming a different one is rejected. A
/// program that changes the frame with `G10`/`G92`/`G51` is also rejected
/// because one fixed offset cannot describe it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCompensationOptions {
    /// Active work-offset XY, in millimetres.
    pub work_offset_xy_mm: [f64; 2],
    /// Maximum XY distance between emitted sub-move endpoints, in
    /// millimetres.
    ///
    /// The app normally passes `min(grid.step_x_mm, grid.step_y_mm)`.
    pub max_step_mm: f64,
    /// Behavior when the callback has no value for a point.
    pub outside: OutsideMap,
}

impl HeightCompensationOptions {
    /// Validates the offset and the subdivision step.
    ///
    /// # Errors
    ///
    /// Returns [`HeightCompensationError::InvalidOptions`] for a non-finite or
    /// out-of-range value. A step below
    /// [`MIN_HEIGHT_MAP_STEP_MM`] or above [`MAX_HEIGHT_MAP_STEP_MM`] is
    /// rejected rather than clamped, so the caller can report the real
    /// problem.
    pub fn validate(&self) -> Result<(), HeightCompensationError> {
        for (axis, value) in ["X", "Y"].iter().zip(self.work_offset_xy_mm) {
            if !value.is_finite() || value.abs() > MAX_COORDINATE_MM {
                return Err(HeightCompensationError::InvalidOptions {
                    message: format!(
                        "work offset {axis} {value} is not a finite value within +/-{MAX_COORDINATE_MM} mm"
                    ),
                });
            }
        }
        if !self.max_step_mm.is_finite()
            || !(MIN_HEIGHT_MAP_STEP_MM..=MAX_HEIGHT_MAP_STEP_MM).contains(&self.max_step_mm)
        {
            return Err(HeightCompensationError::InvalidOptions {
                message: format!(
                    "max_step_mm {} must be finite and between {MIN_HEIGHT_MAP_STEP_MM} and {MAX_HEIGHT_MAP_STEP_MM} mm",
                    self.max_step_mm
                ),
            });
        }
        Ok(())
    }
}

/// Aggregate counters for one transform.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeightCompensationStats {
    /// Source lines, including blank and comment-only lines.
    pub input_lines: usize,
    /// Emitted lines, including copied non-motion lines and generated splits.
    pub output_lines: usize,
    /// Moving input blocks that were rewritten.
    ///
    /// Non-moving `G0`/`G1` blocks, `F`-only lines, and modal-only lines are
    /// not counted.
    pub input_motion_blocks: usize,
    /// Emitted motion blocks, one per sub-move or sub-arc.
    pub output_motion_blocks: usize,
    /// Moving input blocks that produced more than one output motion block.
    pub split_motions: usize,
    /// Emitted motion blocks whose endpoint received a compensation value.
    pub compensated_motions: usize,
    /// Emitted motion blocks whose endpoint had no value and kept its
    /// programmed Z under [`OutsideMap::Leave`].
    pub uncompensated_motions: usize,
    /// Rewritten input arc blocks.
    pub arc_motions: usize,
    /// Largest XY distance between consecutive emitted motion endpoints, in
    /// millimetres.
    pub max_output_step_mm: f64,
    /// Lowest applied compensation, in millimetres.
    pub min_compensation_mm: Option<f64>,
    /// Highest applied compensation, in millimetres.
    pub max_compensation_mm: Option<f64>,
}

/// A completed height-map rewrite.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightCompensatedProgram {
    /// Rewritten program text. Lines end with `\n`.
    pub source: String,
    /// Counters describing what was rewritten.
    pub stats: HeightCompensationStats,
    /// Non-fatal issues, such as dropped `N` words or uncompensated moves.
    pub warnings: Vec<HeightCompensationWarning>,
}

impl HeightCompensatedProgram {
    /// Returns `true` when every emitted motion endpoint received a height
    /// value.
    ///
    /// This is `false` only after an [`OutsideMap::Leave`] transform that kept
    /// one or more programmed Z values. Under [`OutsideMap::Reject`] a
    /// successful result is always fully compensated.
    #[must_use]
    pub fn is_fully_compensated(&self) -> bool {
        self.stats.uncompensated_motions == 0
    }
}

/// Classification of a [`HeightCompensationWarning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeightCompensationWarningKind {
    /// The program used `N` line numbers; all were removed from the output.
    LineNumbersDropped,
    /// One or more endpoints on this line were outside the map and were left
    /// uncompensated under [`OutsideMap::Leave`].
    OutsideMap,
}

/// A non-fatal issue found while rewriting the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeightCompensationWarning {
    /// 1-based source line, or `0` for a program-wide warning.
    pub line: usize,
    /// Machine-readable warning class.
    pub kind: HeightCompensationWarningKind,
    /// Human-readable explanation.
    pub message: String,
}

/// A fatal height-map transform error.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum HeightCompensationError {
    /// An option is non-finite or outside its supported range.
    #[error("invalid height-compensation options: {message}")]
    InvalidOptions {
        /// Explanation naming the bad field and value.
        message: String,
    },
    /// The input program is not valid G-code.
    #[error(
        "the program could not be parsed ({} error(s)); fix the program before applying the height map",
        errors.len()
    )]
    Parse {
        /// All parser diagnostics, sorted by line and column.
        errors: Vec<ParseError>,
    },
    /// The program uses a construct that cannot be represented safely.
    #[error("line {line}: {message}")]
    UnsupportedConstruct {
        /// 1-based source line.
        line: usize,
        /// Short stable tag for the construct, for example `"G53"`.
        construct: &'static str,
        /// Explanation for the operator.
        message: String,
    },
    /// A move was outside the height map and [`OutsideMap::Reject`] was set.
    #[error(
        "line {line}: machine XY ({x_mm:.4}, {y_mm:.4}) mm is outside the height map; the transform was rejected"
    )]
    OutsideMap {
        /// 1-based source line.
        line: usize,
        /// Machine X of the missing sample, in millimetres.
        x_mm: f64,
        /// Machine Y of the missing sample, in millimetres.
        y_mm: f64,
    },
    /// The callback returned a non-finite or absurdly large delta.
    #[error(
        "line {line}: the height callback returned {value_mm} mm at machine XY ({x_mm:.4}, {y_mm:.4}); values must be finite and within +/-{MAX_COORDINATE_MM} mm"
    )]
    InvalidCompensation {
        /// 1-based source line.
        line: usize,
        /// Machine X of the sample, in millimetres.
        x_mm: f64,
        /// Machine Y of the sample, in millimetres.
        y_mm: f64,
        /// The invalid value.
        value_mm: f64,
    },
    /// A compensated Z would be outside the parser's coordinate range.
    #[error(
        "line {line}: compensated Z {z_mm} mm is outside the parser's +/-{MAX_COORDINATE_MM} mm range"
    )]
    CoordinateOutOfRange {
        /// 1-based source line.
        line: usize,
        /// The composed Z value, in millimetres.
        z_mm: f64,
    },
    /// The rewritten program would exceed the output-line safety cap.
    #[error("the transformed program would exceed {limit} output lines")]
    ProgramTooLarge {
        /// The cap, [`MAX_COMPENSATED_OUTPUT_LINES`].
        limit: usize,
    },
}

impl HeightCompensationError {
    /// Returns the parser diagnostics when this is a [`Self::Parse`] error.
    #[must_use]
    pub fn parse_errors(&self) -> Option<&[ParseError]> {
        match self {
            Self::Parse { errors } => Some(errors),
            _ => None,
        }
    }
}

/// Rewrites `source` with sender-side height-map compensation.
///
/// `compensation` receives machine XY in millimetres and returns a Z delta in
/// millimetres, or `None` when the point is outside the map or its cell is
/// invalid. See the module documentation for the exact supported constructs.
///
/// The input is fully parsed first with [`ParserConfig::default`](crate::ParserConfig::default);
/// a malformed program returns [`HeightCompensationError::Parse`] and never
/// panics.
///
/// # Errors
///
/// Returns a typed error for invalid options, parse failures, unsupported
/// constructs, a missing sample under [`OutsideMap::Reject`], an invalid
/// callback value, or an output that exceeds the safety cap.
pub fn apply_height_compensation<F>(
    source: &str,
    options: &HeightCompensationOptions,
    compensation: F,
) -> Result<HeightCompensatedProgram, HeightCompensationError>
where
    F: Fn(f64, f64) -> Option<f64>,
{
    options.validate()?;
    let program =
        parse_program(source).map_err(|errors| HeightCompensationError::Parse { errors })?;
    let mut parser = Parser::new();
    let mut transform = Transform {
        options,
        compensation,
        output: Vec::new(),
        stats: HeightCompensationStats::default(),
        warnings: Vec::new(),
        wcs: None,
        applied_comp_mm: 0.0,
        dropped_numbers: false,
    };

    for parsed in &program.lines {
        transform.process_line(parsed, &mut parser)?;
    }

    transform.stats.input_lines = program.lines.len();
    transform.stats.output_lines = transform.output.len();
    if transform.dropped_numbers {
        transform.warnings.insert(
            0,
            HeightCompensationWarning {
                line: 0,
                kind: HeightCompensationWarningKind::LineNumbersDropped,
                message:
                    "N line numbers were removed because split blocks cannot keep one unique N word"
                        .to_string(),
            },
        );
    }
    let mut output = transform.output.join("\n");
    if !transform.output.is_empty() {
        output.push('\n');
    }
    Ok(HeightCompensatedProgram {
        source: output,
        stats: transform.stats,
        warnings: transform.warnings,
    })
}

/// Mutable state of one rewrite.
struct Transform<'a, F> {
    options: &'a HeightCompensationOptions,
    compensation: F,
    output: Vec<String>,
    stats: HeightCompensationStats,
    warnings: Vec<HeightCompensationWarning>,
    wcs: Option<Wcs>,
    /// Compensation applied at the last emitted endpoint, in millimetres.
    ///
    /// The controller's current Z is `nominal Z + applied_comp_mm` in the
    /// current program frame. Keeping the delta rather than an absolute Z
    /// means a preserved `G43.1` frame change is handled correctly.
    applied_comp_mm: f64,
    dropped_numbers: bool,
}

impl<F> Transform<'_, F>
where
    F: Fn(f64, f64) -> Option<f64>,
{
    /// Validates, plans, rewrites, and then replays one source line.
    fn process_line(
        &mut self,
        parsed: &ProgramLine,
        parser: &mut Parser,
    ) -> Result<(), HeightCompensationError> {
        if let Some(_checksum) = parsed.checksum {
            return Err(unsupported(
                parsed.line,
                "checksum",
                "checksummed input is not supported by height-map compensation",
            ));
        }
        if parsed.words.iter().any(|word| word.letter == 'N') {
            self.dropped_numbers = true;
        }
        if parsed.words.is_empty() {
            return self.copy_source(parsed);
        }

        let lexed = LexedLine {
            words: parsed.words.clone(),
            comments: parsed.comments.clone(),
            block_delete: parsed.block_delete,
            checksum: parsed.checksum,
            percent: false,
        };
        let block = Block::parse(parsed.line, &lexed)
            .map_err(|errors| HeightCompensationError::Parse { errors })?;
        let start = parser.state().position;
        let plan = {
            let planner = Planner {
                state: *parser.state(),
                offsets: *parser.offsets(),
                homes: *parser.homes(),
                machine: parser.machine_position(),
                config: parser.config(),
                line: parsed.line,
            };
            planner
                .plan(&block)
                .map_err(|errors| HeightCompensationError::Parse { errors })?
        };

        self.check_supported(parsed.line, &block)?;
        self.rewrite_block(parsed, &block, &plan, start)?;

        parser
            .parse_lexed(parsed.line, &parsed.source, lexed)
            .map_err(|errors| HeightCompensationError::Parse { errors })?;
        Ok(())
    }

    /// Rejects constructs whose coordinates or frame this pass cannot model.
    fn check_supported(
        &mut self,
        line: usize,
        block: &Block,
    ) -> Result<(), HeightCompensationError> {
        for word in &block.g {
            match word.code {
                GCode::MachineCoords => {
                    return Err(unsupported(
                        line,
                        "G53",
                        "G53 machine-coordinate motion cannot be height-compensated",
                    ));
                }
                GCode::Home | GCode::SecondHome => {
                    return Err(unsupported(
                        line,
                        "G28/G30",
                        "G28/G30 home motion targets machine coordinates and cannot be height-compensated",
                    ));
                }
                GCode::Canned(kind) => {
                    return Err(unsupported(
                        line,
                        "canned cycle",
                        format!(
                            "{} canned cycles cannot be height-compensated; expand the cycle first",
                            kind.g_code()
                        ),
                    ));
                }
                GCode::Probe(kind) => {
                    return Err(unsupported(
                        line,
                        "probing",
                        format!(
                            "{} probing moves are never height-compensated; apply the map to the part program, not a probe cycle",
                            kind.g_code()
                        ),
                    ));
                }
                GCode::SetCoordinateSystem => {
                    return Err(unsupported(
                        line,
                        "G10",
                        "G10 work-offset changes are not supported because the caller supplies one fixed offset",
                    ));
                }
                GCode::SetOffsets
                | GCode::ClearOffsets
                | GCode::SuspendOffsets
                | GCode::RestoreOffsets => {
                    return Err(unsupported(
                        line,
                        "G92",
                        "G92 offset changes are not supported because the caller supplies one fixed offset",
                    ));
                }
                GCode::ScalingSet => {
                    return Err(unsupported(
                        line,
                        "G51",
                        "G51 scaling changes the XY mapping and is not supported by height-map compensation",
                    ));
                }
                GCode::FeedInverseTime => {
                    return Err(unsupported(
                        line,
                        "G93",
                        "G93 inverse-time feed cannot be split without recomputing the feed per sub-move",
                    ));
                }
                GCode::Wcs(wcs) => match self.wcs {
                    None => self.wcs = Some(wcs),
                    Some(current) if current != wcs => {
                        return Err(unsupported(
                            line,
                            "work coordinate system",
                            format!(
                                "the program switches from {} to {}; one work offset was supplied",
                                current.as_str(),
                                wcs.as_str()
                            ),
                        ));
                    }
                    Some(_) => {}
                },
                _ => {}
            }
        }
        Ok(())
    }

    /// Rewrites one planned block, or copies it when it does not move.
    fn rewrite_block(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        plan: &Plan,
        start: [f64; 3],
    ) -> Result<(), HeightCompensationError> {
        if plan.moves.is_empty() {
            return self.copy_source(parsed);
        }
        match plan.state.motion {
            MotionMode::Rapid | MotionMode::Linear => {
                let [PlannedMove::Line { to_work, .. }] = plan.moves.as_slice() else {
                    return Err(unsupported(
                        parsed.line,
                        "motion",
                        "the block produced more than one straight move; only G0/G1 can be height-compensated",
                    ));
                };
                if distance3(start, *to_work) <= EPSILON_MM {
                    // A non-moving G0/G1 is not compensated (ioSender #451).
                    return self.copy_source(parsed);
                }
                let code = if plan.state.motion == MotionMode::Rapid {
                    "G0"
                } else {
                    "G1"
                };
                let style = self.line_style(parsed, block, code);
                self.rewrite_line(parsed.line, style, &plan.state, start, *to_work)
            }
            MotionMode::ArcCw | MotionMode::ArcCcw => {
                let [
                    PlannedMove::Arc {
                        to_work,
                        center_work,
                        plane,
                        clockwise,
                    },
                ] = plan.moves.as_slice()
                else {
                    return Err(unsupported(
                        parsed.line,
                        "motion",
                        "the block produced more than one arc; only a single G2/G3 can be height-compensated",
                    ));
                };
                if *plane != Plane::Xy {
                    return Err(unsupported(
                        parsed.line,
                        "G18/G19 arc",
                        format!(
                            "{} arcs are not supported; height-map compensation is defined for G17 XY arcs only",
                            plane.g_code()
                        ),
                    ));
                }
                if (to_work[2] - start[2]).abs() > EPSILON_MM {
                    return Err(unsupported(
                        parsed.line,
                        "helical arc",
                        "helical input arcs are not supported; only planar G17 arcs can be compensated",
                    ));
                }
                let code = if plan.state.motion == MotionMode::ArcCw {
                    "G2"
                } else {
                    "G3"
                };
                let style = self.line_style(parsed, block, code);
                self.rewrite_arc(
                    parsed.line,
                    style,
                    &plan.state,
                    ArcMotion {
                        start,
                        to: *to_work,
                        center: *center_work,
                        clockwise: *clockwise,
                    },
                )
            }
            MotionMode::Probe(_) => Err(unsupported(
                parsed.line,
                "probing",
                "probing moves are never height-compensated",
            )),
            MotionMode::Canned(_) => Err(unsupported(
                parsed.line,
                "canned cycle",
                "canned cycles cannot be height-compensated; expand the cycle first",
            )),
            MotionMode::None => self.copy_source(parsed),
        }
    }

    /// Emits one straight move as contiguous sub-moves.
    fn rewrite_line(
        &mut self,
        line: usize,
        style: MotionStyle,
        state: &ParserState,
        start: [f64; 3],
        to: [f64; 3],
    ) -> Result<(), HeightCompensationError> {
        let dx = to[0] - start[0];
        let dy = to[1] - start[1];
        let dz = to[2] - start[2];
        let xy = dx.hypot(dy);
        let count = if xy <= EPSILON_MM {
            1
        } else {
            self.line_subdivisions(xy)?
        };

        self.emit_comments(&style)?;
        self.stats.input_motion_blocks += 1;
        if count > 1 {
            self.stats.split_motions += 1;
        }

        let mut previous = start;
        let mut previous_comp = self.applied_comp_mm;
        let mut outside = 0usize;
        for index in 1..=count {
            #[allow(clippy::cast_precision_loss)]
            let fraction = index as f64 / count as f64;
            let nominal = if index == count {
                to
            } else {
                [
                    start[0] + dx * fraction,
                    start[1] + dy * fraction,
                    start[2] + dz * fraction,
                ]
            };
            let sampled = self.sample(line, [nominal[0], nominal[1]])?;
            let (delta, compensated) = match sampled {
                Some(value) => {
                    self.record_compensation(value);
                    (value, true)
                }
                None => {
                    outside += 1;
                    (0.0, false)
                }
            };
            let emitted = [nominal[0], nominal[1], nominal[2] + delta];
            self.check_emitted_z(line, emitted[2])?;
            let coordinates = self.coordinates(
                state.units,
                state.distance,
                previous,
                emitted,
                previous_comp,
            );
            let first = index == 1;
            let last = index == count;
            let text = style.render(first, last, &coordinates);
            self.push_line(text)?;

            self.stats.output_motion_blocks += 1;
            if compensated {
                self.stats.compensated_motions += 1;
            } else {
                self.stats.uncompensated_motions += 1;
            }
            self.observe_step(previous, nominal);
            previous = nominal;
            previous_comp = delta;
            self.applied_comp_mm = delta;
        }

        if outside > 0 {
            self.warnings.push(HeightCompensationWarning {
                line,
                kind: HeightCompensationWarningKind::OutsideMap,
                message: format!(
                    "{outside} of {count} endpoint(s) were outside the height map and kept their programmed Z"
                ),
            });
        }
        Ok(())
    }

    /// Emits one planar arc as contiguous sub-arcs with incremental I/J.
    fn rewrite_arc(
        &mut self,
        line: usize,
        style: MotionStyle,
        state: &ParserState,
        arc: ArcMotion,
    ) -> Result<(), HeightCompensationError> {
        let ArcMotion {
            start,
            to,
            center,
            clockwise,
        } = arc;
        let radius = (start[0] - center[0]).hypot(start[1] - center[1]);
        if !radius.is_finite() || radius <= EPSILON_MM {
            return Err(unsupported(
                line,
                "arc",
                "the arc has no usable radius after planning",
            ));
        }
        let start_angle = (start[1] - center[1]).atan2(start[0] - center[0]);
        let end_angle = (to[1] - center[1]).atan2(to[0] - center[0]);
        let sweep = directed_sweep(start_angle, end_angle, clockwise);
        let total = sweep.abs();
        if !total.is_finite() || total <= EPSILON_MM {
            return Err(unsupported(line, "arc", "the arc has no usable sweep"));
        }
        let count = self.arc_subdivisions(total, radius)?;
        #[allow(clippy::cast_precision_loss)]
        let sub_sweep = total / count as f64;
        if !sub_sweep.is_finite() || sub_sweep <= 1.0e-9 {
            // The parser treats a directed sweep of at most 1e-9 rad as a full
            // circle, so a smaller sub-arc would silently become one.
            return Err(unsupported(
                line,
                "arc",
                "an arc sub-move is below the angular output resolution",
            ));
        }

        self.emit_comments(&style)?;
        self.stats.input_motion_blocks += 1;
        self.stats.arc_motions += 1;
        if count > 1 {
            self.stats.split_motions += 1;
        }

        let mut previous = start;
        let mut previous_comp = self.applied_comp_mm;
        let mut outside = 0usize;
        for index in 1..=count {
            #[allow(clippy::cast_precision_loss)]
            let fraction = index as f64 / count as f64;
            let (nominal_xy, nominal_z) = if index == count {
                ([to[0], to[1]], to[2])
            } else {
                let angle = start_angle + sweep * fraction;
                (
                    [
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ],
                    start[2],
                )
            };
            let sampled = self.sample(line, nominal_xy)?;
            let (delta, compensated) = match sampled {
                Some(value) => {
                    self.record_compensation(value);
                    (value, true)
                }
                None => {
                    outside += 1;
                    (0.0, false)
                }
            };
            let emitted = [nominal_xy[0], nominal_xy[1], nominal_z + delta];
            self.check_emitted_z(line, emitted[2])?;
            let mut coordinates = self.coordinates(
                state.units,
                state.distance,
                previous,
                emitted,
                previous_comp,
            );
            // I/J are always incremental from this sub-arc's start.
            coordinates.push(format!(
                "I{}",
                format_units(state.units, center[0] - previous[0])
            ));
            coordinates.push(format!(
                "J{}",
                format_units(state.units, center[1] - previous[1])
            ));
            if arc_delta_is_zero(state.distance, state.units, previous, emitted) {
                return Err(unsupported(
                    line,
                    "arc",
                    "a sub-arc endpoint is below the output resolution; the arc cannot be split safely",
                ));
            }
            let first = index == 1;
            let last = index == count;
            let text = style.render(first, last, &coordinates);
            self.push_line(text)?;

            self.stats.output_motion_blocks += 1;
            if compensated {
                self.stats.compensated_motions += 1;
            } else {
                self.stats.uncompensated_motions += 1;
            }
            self.observe_step(previous, [nominal_xy[0], nominal_xy[1], nominal_z]);
            previous = [nominal_xy[0], nominal_xy[1], nominal_z];
            previous_comp = delta;
            self.applied_comp_mm = delta;
        }

        if outside > 0 {
            self.warnings.push(HeightCompensationWarning {
                line,
                kind: HeightCompensationWarningKind::OutsideMap,
                message: format!(
                    "{outside} of {count} arc endpoint(s) were outside the height map and kept their programmed Z"
                ),
            });
        }
        Ok(())
    }

    /// Number of sub-moves for an XY distance.
    fn line_subdivisions(&self, distance_mm: f64) -> Result<usize, HeightCompensationError> {
        let count = (distance_mm / self.options.max_step_mm - 1.0e-12).ceil();
        if !count.is_finite() || count < 1.0 || count > MAX_COMPENSATED_OUTPUT_LINES as f64 {
            return Err(HeightCompensationError::ProgramTooLarge {
                limit: MAX_COMPENSATED_OUTPUT_LINES,
            });
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut count = count as usize;
        #[allow(clippy::cast_precision_loss)]
        if distance_mm / (count as f64) > self.options.max_step_mm {
            count = count.saturating_add(1);
        }
        if count > MAX_COMPENSATED_OUTPUT_LINES {
            return Err(HeightCompensationError::ProgramTooLarge {
                limit: MAX_COMPENSATED_OUTPUT_LINES,
            });
        }
        Ok(count.max(1))
    }

    /// Number of sub-arcs whose chord stays within the step limit.
    fn arc_subdivisions(
        &self,
        total_sweep_rad: f64,
        radius_mm: f64,
    ) -> Result<usize, HeightCompensationError> {
        let ratio = (self.options.max_step_mm / (2.0 * radius_mm)).clamp(0.0, 1.0);
        let angle_step = (2.0 * ratio.asin()).max(1.0e-12);
        let mut count = (total_sweep_rad / angle_step - 1.0e-12).ceil().max(1.0);
        if !count.is_finite() {
            return Err(HeightCompensationError::ProgramTooLarge {
                limit: MAX_COMPENSATED_OUTPUT_LINES,
            });
        }
        // A full circle must never become one sub-arc: its endpoint would be
        // indistinguishable from its start in incremental mode.
        if total_sweep_rad >= TAU - 1.0e-9 {
            count = count.max(2.0);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = count as usize;
        if count > MAX_COMPENSATED_OUTPUT_LINES {
            return Err(HeightCompensationError::ProgramTooLarge {
                limit: MAX_COMPENSATED_OUTPUT_LINES,
            });
        }
        Ok(count.max(1))
    }

    /// Samples the callback in machine coordinates.
    fn sample(
        &self,
        line: usize,
        work_xy: [f64; 2],
    ) -> Result<Option<f64>, HeightCompensationError> {
        let x_mm = work_xy[0] + self.options.work_offset_xy_mm[0];
        let y_mm = work_xy[1] + self.options.work_offset_xy_mm[1];
        match (self.compensation)(x_mm, y_mm) {
            Some(value) if !value.is_finite() || value.abs() > MAX_COORDINATE_MM => {
                Err(HeightCompensationError::InvalidCompensation {
                    line,
                    x_mm,
                    y_mm,
                    value_mm: value,
                })
            }
            Some(value) => Ok(Some(value)),
            None if self.options.outside == OutsideMap::Reject => {
                Err(HeightCompensationError::OutsideMap { line, x_mm, y_mm })
            }
            None => Ok(None),
        }
    }

    /// Rejects a composed Z that the emitted program could not reparse.
    fn check_emitted_z(&self, line: usize, z_mm: f64) -> Result<(), HeightCompensationError> {
        if !z_mm.is_finite() || z_mm.abs() > MAX_COORDINATE_MM {
            return Err(HeightCompensationError::CoordinateOutOfRange { line, z_mm });
        }
        Ok(())
    }

    /// Records an applied compensation for the stats range.
    fn record_compensation(&mut self, value: f64) {
        self.stats.min_compensation_mm = Some(match self.stats.min_compensation_mm {
            Some(current) => current.min(value),
            None => value,
        });
        self.stats.max_compensation_mm = Some(match self.stats.max_compensation_mm {
            Some(current) => current.max(value),
            None => value,
        });
    }

    /// Updates the largest observed XY step.
    fn observe_step(&mut self, from: [f64; 3], to: [f64; 3]) {
        let step = (to[0] - from[0]).hypot(to[1] - from[1]);
        if step > self.stats.max_output_step_mm {
            self.stats.max_output_step_mm = step;
        }
    }

    /// Builds the coordinate words for one emitted point.
    fn coordinates(
        &self,
        units: Units,
        distance: DistanceMode,
        previous: [f64; 3],
        emitted: [f64; 3],
        previous_comp_mm: f64,
    ) -> Vec<String> {
        match distance {
            DistanceMode::Absolute => vec![
                format!("X{}", format_units(units, emitted[0])),
                format!("Y{}", format_units(units, emitted[1])),
                format!("Z{}", format_units(units, emitted[2])),
            ],
            DistanceMode::Incremental => {
                let previous_z = previous[2] + previous_comp_mm;
                vec![
                    format!("X{}", format_units(units, emitted[0] - previous[0])),
                    format!("Y{}", format_units(units, emitted[1] - previous[1])),
                    format!("Z{}", format_units(units, emitted[2] - previous_z)),
                ]
            }
        }
    }

    /// Copies a source line, removing its `N` word when present.
    fn copy_source(&mut self, parsed: &ProgramLine) -> Result<(), HeightCompensationError> {
        let text = match parsed.words.iter().find(|word| word.letter == 'N') {
            Some(word) => strip_line_number(&parsed.source, word.column),
            None => parsed.source.clone(),
        };
        self.push_line(text)
    }

    /// Emits comments as standalone semicolon-comment lines.
    fn emit_comments(&mut self, style: &MotionStyle) -> Result<(), HeightCompensationError> {
        for comment in &style.comments {
            self.push_line(format!("; {comment}"))?;
        }
        Ok(())
    }

    /// Builds the fixed parts of the output motion lines.
    fn line_style(
        &self,
        parsed: &ProgramLine,
        block: &Block,
        motion_code: &'static str,
    ) -> MotionStyle {
        let mut modal_g = Vec::new();
        let mut motion_seen = false;
        for word in &block.g {
            if word.code.group() == GGroup::Motion {
                if !motion_seen {
                    modal_g.push(motion_code.to_string());
                    motion_seen = true;
                }
            } else {
                modal_g.push(format!("G{}", format_decimal(word.value)));
            }
        }
        if !motion_seen {
            modal_g.push(motion_code.to_string());
        }
        let mut first_words = Vec::new();
        let mut last_words = Vec::new();
        let mut feed = None;
        // Keep source order so `S`/`M3` ordering is not changed.
        let mut m_index = 0usize;
        for word in &parsed.words {
            match word.letter {
                'F' => feed = Some(format!("F{}", format_decimal(word.value))),
                'S' | 'T' => {
                    first_words.push(format!("{}{}", word.letter, format_decimal(word.value)));
                }
                'M' => {
                    let Some(m_word) = block.m.get(m_index) else {
                        continue;
                    };
                    m_index += 1;
                    let text = format!("M{}", format_decimal(m_word.value));
                    match m_word.code {
                        MCode::SpindleClockwise
                        | MCode::SpindleCounterClockwise
                        | MCode::MistOn
                        | MCode::FloodOn => first_words.push(text),
                        MCode::SpindleStop
                        | MCode::CoolantOff
                        | MCode::ProgramStop
                        | MCode::OptionalStop
                        | MCode::ProgramEnd
                        | MCode::ProgramEndRewind
                        | MCode::ToolChange => last_words.push(text),
                    }
                }
                _ => {}
            }
        }
        MotionStyle {
            block_delete: parsed.block_delete,
            modal_g,
            motion_code,
            first_words,
            feed,
            last_words,
            comments: parsed.comments.clone(),
        }
    }

    /// Pushes one output line or fails at the safety cap.
    fn push_line(&mut self, text: String) -> Result<(), HeightCompensationError> {
        if self.output.len() >= MAX_COMPENSATED_OUTPUT_LINES {
            return Err(HeightCompensationError::ProgramTooLarge {
                limit: MAX_COMPENSATED_OUTPUT_LINES,
            });
        }
        self.output.push(text);
        Ok(())
    }
}

/// One planned planar arc to rewrite.
#[derive(Debug, Clone, Copy)]
struct ArcMotion {
    start: [f64; 3],
    to: [f64; 3],
    center: [f64; 3],
    clockwise: bool,
}

/// Fixed tokens shared by every sub-line of one input block.
struct MotionStyle {
    block_delete: bool,
    /// Non-motion words followed by the motion code, for the first sub-line.
    modal_g: Vec<String>,
    /// The resolved motion code, repeated on continuation lines.
    motion_code: &'static str,
    /// Words that take effect before the motion: `S`, `T`, spindle start, and
    /// coolant on.
    first_words: Vec<String>,
    /// `F` word, emitted once on the first sub-line.
    feed: Option<String>,
    /// Words that take effect after the motion: spindle/coolant stop, tool
    /// change, and program flow.
    last_words: Vec<String>,
    comments: Vec<String>,
}

impl MotionStyle {
    /// Renders one sub-line.
    fn render(&self, first: bool, last: bool, coordinates: &[String]) -> String {
        let mut line = String::new();
        if self.block_delete {
            line.push('/');
        }
        if first {
            line.push_str(&self.modal_g.join(" "));
            for word in &self.first_words {
                line.push(' ');
                line.push_str(word);
            }
        } else {
            line.push_str(self.motion_code);
        }
        for coordinate in coordinates {
            line.push(' ');
            line.push_str(coordinate);
        }
        if first && let Some(feed) = &self.feed {
            line.push(' ');
            line.push_str(feed);
        }
        if last {
            for word in &self.last_words {
                line.push(' ');
                line.push_str(word);
            }
        }
        if line.is_empty() {
            // A motion block always has at least its G code, but keep the
            // output parseable if a future caller changes that invariant.
            line.push_str(self.motion_code);
        }
        line
    }
}

/// Builds one unsupported-construct error.
fn unsupported(
    line: usize,
    construct: &'static str,
    message: impl Into<String>,
) -> HeightCompensationError {
    HeightCompensationError::UnsupportedConstruct {
        line,
        construct,
        message: message.into(),
    }
}

/// Euclidean distance between two 3D points.
fn distance3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Directed arc sweep, treating a zero sweep as a full circle.
fn directed_sweep(start: f64, end: f64, clockwise: bool) -> f64 {
    let mut delta = if clockwise { start - end } else { end - start };
    delta = delta.rem_euclid(TAU);
    if !delta.is_finite() {
        return 0.0;
    }
    if delta <= 1.0e-9 {
        delta = TAU;
    }
    if clockwise { -delta } else { delta }
}

/// Returns `true` when an arc sub-move would collapse to zero in the output.
fn arc_delta_is_zero(
    distance: DistanceMode,
    units: Units,
    previous: [f64; 3],
    emitted: [f64; 3],
) -> bool {
    match distance {
        DistanceMode::Absolute => {
            format_units(units, emitted[0]) == format_units(units, previous[0])
                && format_units(units, emitted[1]) == format_units(units, previous[1])
        }
        DistanceMode::Incremental => {
            format_units(units, emitted[0] - previous[0]) == "0"
                && format_units(units, emitted[1] - previous[1]) == "0"
        }
    }
}

/// Formats a millimetre value in program units with six decimal places.
fn format_units(units: Units, value_mm: f64) -> String {
    format_decimal(units.from_mm(value_mm))
}

/// Formats a decimal without an exponent, trailing zeros, or `-0`.
fn format_decimal(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    if text == "0" && value != 0.0 {
        // Keep a small but meaningful value, such as a very low feed, instead
        // of rounding it to zero.
        text = format!("{value:.12}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        if text == "-0" {
            text = "0".to_string();
        }
    }
    text
}

/// Removes the leading `N` word from a source line.
///
/// `column` is the 1-based byte column of the `N` letter, as reported by the
/// lexer. Leading whitespace and the rest of the line are preserved.
fn strip_line_number(source: &str, column: usize) -> String {
    let start = column.saturating_sub(1);
    let bytes = source.as_bytes();
    if start >= bytes.len() || !matches!(bytes[start], b'N' | b'n') {
        return source.to_string();
    }
    let mut end = start + 1;
    while end < bytes.len() && matches!(bytes[end], b'0'..=b'9' | b'+' | b'-' | b'.') {
        end += 1;
    }
    let mut text = String::with_capacity(source.len().saturating_sub(end - start));
    text.push_str(&source[..start]);
    text.push_str(&source[end..]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_format_is_plain_and_trimmed() {
        assert_eq!(format_decimal(1.5), "1.5");
        assert_eq!(format_decimal(-0.0), "0");
        assert_eq!(format_decimal(10.0), "10");
        assert_eq!(format_decimal(f64::NAN), "0");
    }

    #[test]
    fn options_reject_bad_steps_and_offsets() {
        let options = HeightCompensationOptions {
            work_offset_xy_mm: [0.0, 0.0],
            max_step_mm: 0.0,
            outside: OutsideMap::Leave,
        };
        assert!(matches!(
            options.validate(),
            Err(HeightCompensationError::InvalidOptions { .. })
        ));
        let options = HeightCompensationOptions {
            work_offset_xy_mm: [f64::NAN, 0.0],
            max_step_mm: 1.0,
            outside: OutsideMap::Leave,
        };
        assert!(options.validate().is_err());
    }

    #[test]
    fn line_numbers_are_stripped_without_touching_spacing() {
        assert_eq!(strip_line_number("N10 G0 X1", 1), " G0 X1");
        assert_eq!(strip_line_number("/N10 G0 X1", 2), "/ G0 X1");
        assert_eq!(strip_line_number("G0 X1", 1), "G0 X1");
    }
}
