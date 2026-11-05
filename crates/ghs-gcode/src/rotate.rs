//! Sender-side rotation of a whole program about an XY pivot.
//!
//! This is a pure source-to-source rewrite, like
//! [`apply_height_compensation`](crate::apply_height_compensation), not a live
//! stream correction. A caller parses the source once, rotates it, and loads
//! the result for preview or a job. The original program is never mutated, so
//! the app can keep it for Revert.
//!
//! # Algorithm
//!
//! Every explicit absolute XY endpoint of a `G0`/`G1` move and every `G17`
//! `G2`/`G3` arc is rotated counter-clockwise (looking down +Z) by
//! [`RotationOptions::angle_deg`] about [`RotationOptions::pivot_mm`]. `I`/`J`
//! center offsets are incremental vectors, so they are rotated by the same
//! angle without a pivot translation. An arc described with `R` is resolved to
//! its center and re-emitted with `I`/`J`, with a
//! [`RotationWarningKind::RadiusArcResolved`] warning.
//!
//! The pivot is in the same **program (work)** frame as the source endpoints;
//! work offsets configured on the controller are not applied. All geometry is
//! converted to millimetres, rotated there, and emitted in the active
//! `G20`/`G21` unit mode. A program in inches therefore stays in inches; it is
//! never silently converted. Coordinates use up to six decimal places in
//! program units with trailing zeros trimmed, matching the height-map
//! formatter.
//!
//! A block with no explicit X or Y word (a `Z`-only move, `F`-only line,
//! comment, blank line, tool change, or modal line) is copied byte for byte. A
//! motion block is rewritten in place: its `N` word, modal words, `Z`, `F`,
//! `S`, `T`, comments, spacing, lowercase letters, and block-delete marker are
//! left untouched. An absolute XY endpoint is rotated even when the source
//! block happens to be a non-move, so a return to the programmed origin lands
//! on the rotated origin. Because no line is split, `N` line numbers stay
//! valid.
//!
//! At exactly `0.0` degrees the validated source is returned unchanged.
//!
//! # Preserved constructs
//!
//! Comments, blank lines, block-delete markers, `N`, `M3`/`M4`/`M5`,
//! `M7`/`M8`/`M9`, `S`, `T`, `M6`, `M0`/`M1`/`M2`/`M30`, `G4`, `G20`/`G21`,
//! `G90`, `G17`, `G43.1`/`G49`, `G50`, `G61`/`G64`, `G98`/`G99`, and program
//! structure. A `G54`-`G59.3` selection is accepted while it stays on one
//! coordinate system; switching to a different one is rejected.
//!
//! # Rejected constructs
//!
//! The following return [`RotationError::UnsupportedConstruct`]; a rejected
//! program produces no output and the caller's source is not touched:
//!
//! - `G91` incremental motion: rotating an incremental vector without a full
//!   frame model is ambiguous.
//! - `G53` machine-coordinate motion and `G28`/`G30` home moves.
//! - Probing (`G38.x`) and canned cycles.
//! - `G10`, `G92`/`G92.1`-`G92.3`, and `G51` scaling: a source rotation cannot
//!   reproduce a mid-program frame change.
//! - `G18`/`G19` arcs. Straight moves in those planes are fine.
//! - Helical input arcs (a Z change across a `G17` arc).
//! - `G93` inverse-time feed.
//! - Lines carrying a `*` checksum: rewriting invalidates the checksum.
//! - A mid-program switch to a different work coordinate system.
//!
//! Parenthesised expressions, flow control, `G41`/`G42`, and the other codes
//! this crate already rejects fail during the initial parse with
//! [`RotationError::Parse`].
//!
//! # Starting point
//!
//! As with height-map compensation, the controller's current point is not
//! modelled. A program should position XY explicitly before its first cut; a
//! program whose first motion is a `Z`-only or full-circle block inherits the
//! machine's existing XY position. Rotating the source cannot move the tool to
//! the rotated origin.
//!
//! # Limits
//!
//! The angle must be finite and within
//! +/-[`MAX_ROTATION_ANGLE_DEG`]. The pivot must be finite and within
//! +/-[`MAX_COORDINATE_MM`](crate::MAX_COORDINATE_MM). A transformed program
//! may contain at most [`MAX_ROTATED_OUTPUT_LINES`] lines; because no line is
//! split, this bounds the input too. Every emitted program is reparsed before
//! it is returned.

use ghs_core::{Units, Wcs};
use thiserror::Error;

use crate::block::{Block, GCode};
use crate::compensation::format_units;
use crate::config::MAX_COORDINATE_MM;
use crate::error::ParseError;
use crate::lexer::LexedLine;
use crate::parser::{Parser, parse_program};
use crate::planner::{Plan, PlannedMove, Planner};
use crate::program::ProgramLine;
use crate::state::{DistanceMode, MotionMode, ParserState, Plane};

/// Largest accepted absolute rotation angle, in degrees.
///
/// Ten full turns is far beyond any real setup and keeps the trigonometry well
/// away from precision loss.
pub const MAX_ROTATION_ANGLE_DEG: f64 = 3_600.0;

/// Safety cap on output lines.
///
/// Rotation never splits a line, so this also bounds the input program. The cap
/// matches the order of magnitude of the height-map budget while staying small
/// enough that a rewritten program remains practical to stream.
pub const MAX_ROTATED_OUTPUT_LINES: usize = 200_000;

/// Two XY points closer than this are the same point for motion detection.
const EPSILON_MM: f64 = 1.0e-9;

/// Inputs for [`apply_rotation`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationOptions {
    /// Counter-clockwise rotation angle in degrees.
    ///
    /// A negative value rotates clockwise. The absolute value must not exceed
    /// [`MAX_ROTATION_ANGLE_DEG`].
    pub angle_deg: f64,
    /// Pivot point in the XY plane, in millimetres.
    ///
    /// The pivot is in the same **program (work)** frame as the source
    /// endpoints; controller work offsets are not applied. It is always in
    /// millimetres, even for a `G20` program.
    pub pivot_mm: (f64, f64),
}

impl RotationOptions {
    /// Validates the angle and the pivot.
    ///
    /// # Errors
    ///
    /// Returns [`RotationError::InvalidOptions`] for a non-finite value, an
    /// angle beyond +/-[`MAX_ROTATION_ANGLE_DEG`], or a pivot beyond
    /// +/-[`MAX_COORDINATE_MM`](crate::MAX_COORDINATE_MM).
    pub fn validate(&self) -> Result<(), RotationError> {
        if !self.angle_deg.is_finite() || self.angle_deg.abs() > MAX_ROTATION_ANGLE_DEG {
            return Err(RotationError::InvalidOptions {
                message: format!(
                    "angle_deg {} must be finite and within +/-{MAX_ROTATION_ANGLE_DEG} degrees",
                    self.angle_deg
                ),
            });
        }
        for (axis, value) in ["X", "Y"].iter().zip([self.pivot_mm.0, self.pivot_mm.1]) {
            if !value.is_finite() || value.abs() > MAX_COORDINATE_MM {
                return Err(RotationError::InvalidOptions {
                    message: format!(
                        "pivot {axis} {value} is not a finite value within \
                         +/-{MAX_COORDINATE_MM} mm"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Aggregate counters for one rotation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RotationStats {
    /// Source lines, including blank and comment-only lines.
    pub input_lines: usize,
    /// Emitted lines. Rotation never splits or drops a line, so this equals
    /// `input_lines` for a successful non-identity transform.
    pub output_lines: usize,
    /// `G0`/`G1` blocks whose explicit XY endpoint was rewritten.
    ///
    /// A block with no `X` or `Y` word (a `Z`-only move) and a modal-only or
    /// comment line is copied and not counted. A non-moving absolute endpoint
    /// with an `X`/`Y` word is still rewritten.
    pub rotated_moves: usize,
    /// `G2`/`G3` blocks whose endpoint or center was rotated.
    pub rotated_arcs: usize,
}

/// A completed rotation rewrite.
#[derive(Debug, Clone, PartialEq)]
pub struct RotatedProgram {
    /// Rewritten program text. Lines end with `\n`.
    pub source: String,
    /// Counters describing what was rewritten.
    pub stats: RotationStats,
    /// Non-fatal issues, currently an `R` arc resolved to `I`/`J`.
    pub warnings: Vec<RotationWarning>,
}

/// Classification of a [`RotationWarning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RotationWarningKind {
    /// An `R`-form arc was re-emitted with `I`/`J` center offsets.
    RadiusArcResolved,
}

/// A non-fatal issue found while rotating the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationWarning {
    /// 1-based source line, or `0` for a program-wide warning.
    pub line: usize,
    /// Machine-readable warning class.
    pub kind: RotationWarningKind,
    /// Human-readable explanation.
    pub message: String,
}

/// A fatal rotation error.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RotationError {
    /// An option is non-finite or outside its supported range.
    #[error("invalid rotation options: {message}")]
    InvalidOptions {
        /// Explanation naming the bad field and value.
        message: String,
    },
    /// The input program is not valid G-code.
    #[error(
        "the program could not be parsed ({} error(s)); fix the program before rotating it",
        errors.len()
    )]
    Parse {
        /// All parser diagnostics, sorted by line and column.
        errors: Vec<ParseError>,
    },
    /// The program uses a construct that rotation cannot represent safely.
    #[error("line {line}: {message}")]
    UnsupportedConstruct {
        /// 1-based source line.
        line: usize,
        /// Short stable tag for the construct, for example `"G91"`.
        construct: &'static str,
        /// Explanation for the operator.
        message: String,
    },
    /// A rotated XY coordinate would be outside the parser's range.
    #[error("line {line}: rotated XY ({x_mm}, {y_mm}) mm is outside +/-{MAX_COORDINATE_MM} mm")]
    CoordinateOutOfRange {
        /// 1-based source line.
        line: usize,
        /// Rotated X, in millimetres.
        x_mm: f64,
        /// Rotated Y, in millimetres.
        y_mm: f64,
    },
    /// The rewritten program would exceed the output-line safety cap.
    #[error("the rotated program would exceed {limit} output lines")]
    OutputTooLong {
        /// The cap, [`MAX_ROTATED_OUTPUT_LINES`].
        limit: usize,
    },
    /// The rewritten line could not be built, or the output did not reparse.
    ///
    /// This is a safety net for a transform bug and never describes bad input;
    /// a bad source returns [`Self::Parse`] instead.
    #[error("line {line}: the rotated program did not verify: {message}")]
    OutputInvalid {
        /// 1-based source line, or `0` when no location is known.
        line: usize,
        /// Explanation.
        message: String,
    },
}

impl RotationError {
    /// Returns the parser diagnostics when this is a [`Self::Parse`] error.
    #[must_use]
    pub fn parse_errors(&self) -> Option<&[ParseError]> {
        match self {
            Self::Parse { errors } => Some(errors),
            _ => None,
        }
    }
}

/// Rotates a whole program about an XY pivot.
///
/// The input is fully parsed first; a malformed program returns
/// [`RotationError::Parse`] and never panics. See the module documentation for
/// the transform rules, preserved constructs, and the rejection list.
///
/// # Errors
///
/// Returns a typed error for invalid options, parse failures, unsupported
/// constructs, a rotated coordinate outside the parser's range, an output that
/// exceeds the safety cap, or an output that fails to reparse.
pub fn apply_rotation(
    source: &str,
    options: &RotationOptions,
) -> Result<RotatedProgram, RotationError> {
    options.validate()?;
    let program = parse_program(source).map_err(|errors| RotationError::Parse { errors })?;
    if program.lines.len() > MAX_ROTATED_OUTPUT_LINES {
        return Err(RotationError::OutputTooLong {
            limit: MAX_ROTATED_OUTPUT_LINES,
        });
    }

    let (sin, cos) = options.angle_deg.to_radians().sin_cos();
    let identity = options.angle_deg == 0.0;
    let mut transform = Transform {
        options,
        sin,
        cos,
        identity,
        output: Vec::with_capacity(program.lines.len()),
        stats: RotationStats {
            input_lines: program.lines.len(),
            ..RotationStats::default()
        },
        warnings: Vec::new(),
        wcs: None,
    };
    let mut parser = Parser::new();
    for parsed in &program.lines {
        transform.process_line(parsed, &mut parser)?;
    }
    transform.stats.output_lines = transform.output.len();

    let mut output = transform.output.join("\n");
    if !transform.output.is_empty() {
        output.push('\n');
    }
    if !identity && let Err(errors) = parse_program(&output) {
        let (line, message) = errors
            .first()
            .map_or((0, "unknown parse error".to_string()), |error| {
                (error.line, error.message.clone())
            });
        return Err(RotationError::OutputInvalid { line, message });
    }
    Ok(RotatedProgram {
        source: output,
        stats: transform.stats,
        warnings: transform.warnings,
    })
}

/// Mutable state of one rewrite.
struct Transform<'a> {
    options: &'a RotationOptions,
    sin: f64,
    cos: f64,
    identity: bool,
    output: Vec<String>,
    stats: RotationStats,
    warnings: Vec<RotationWarning>,
    wcs: Option<Wcs>,
}

impl Transform<'_> {
    /// Validates, plans, and rewrites one source line.
    fn process_line(
        &mut self,
        parsed: &ProgramLine,
        parser: &mut Parser,
    ) -> Result<(), RotationError> {
        if parsed.checksum.is_some() {
            return Err(unsupported(
                parsed.line,
                "checksum",
                "checksummed input is not supported by rotation; remove the * checksums first",
            ));
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
        let block =
            Block::parse(parsed.line, &lexed).map_err(|errors| RotationError::Parse { errors })?;
        self.check_supported(parsed.line, parser.state(), &block)?;

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
                .map_err(|errors| RotationError::Parse { errors })?
        };
        self.check_plan_supported(parsed.line, &plan, start)?;

        if self.identity {
            self.copy_source(parsed)?;
        } else {
            self.rewrite_block(parsed, &block, &plan, start)?;
        }

        parser
            .parse_lexed(parsed.line, &parsed.source, lexed)
            .map_err(|errors| RotationError::Parse { errors })?;
        Ok(())
    }

    /// Rejects constructs whose coordinates or frame rotation cannot model.
    fn check_supported(
        &mut self,
        line: usize,
        state: &ParserState,
        block: &Block,
    ) -> Result<(), RotationError> {
        for word in &block.g {
            match word.code {
                GCode::Incremental => {
                    return Err(unsupported(
                        line,
                        "G91",
                        "G91 incremental motion is not supported; rotate an absolute (G90) program",
                    ));
                }
                GCode::MachineCoords => {
                    return Err(unsupported(
                        line,
                        "G53",
                        "G53 machine-coordinate motion cannot be rotated",
                    ));
                }
                GCode::Home | GCode::SetHome | GCode::SecondHome | GCode::SetSecondHome => {
                    return Err(unsupported(
                        line,
                        "G28/G30",
                        "G28/G30 home motion targets machine coordinates and cannot be rotated",
                    ));
                }
                GCode::Probe(_) => {
                    return Err(unsupported(
                        line,
                        "G38.x",
                        "probing moves are not rotated; rotate the part program, not a probe cycle",
                    ));
                }
                GCode::Canned(_) => {
                    return Err(unsupported(
                        line,
                        "canned cycle",
                        "canned cycles cannot be rotated; expand the cycle first",
                    ));
                }
                GCode::SetCoordinateSystem => {
                    return Err(unsupported(
                        line,
                        "G10",
                        "G10 work-offset changes are not supported by a source rotation",
                    ));
                }
                GCode::SetOffsets
                | GCode::ClearOffsets
                | GCode::SuspendOffsets
                | GCode::RestoreOffsets => {
                    return Err(unsupported(
                        line,
                        "G92",
                        "G92 offset changes are not supported by a source rotation",
                    ));
                }
                GCode::FeedInverseTime => {
                    return Err(unsupported(
                        line,
                        "G93",
                        "G93 inverse-time feed is not supported by rotation",
                    ));
                }
                GCode::ScalingSet => {
                    return Err(unsupported(
                        line,
                        "G51",
                        "G51 scaling changes the XY mapping and cannot be combined with rotation",
                    ));
                }
                GCode::Wcs(wcs) => match self.wcs {
                    None => self.wcs = Some(wcs),
                    Some(current) if current != wcs => {
                        return Err(unsupported(
                            line,
                            "work coordinate system",
                            format!(
                                "the program switches from {} to {}; rotation supports one \
                                 work coordinate system",
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

        let plane = block
            .g
            .iter()
            .find_map(|word| match word.code {
                GCode::PlaneXy => Some(Plane::Xy),
                GCode::PlaneZx => Some(Plane::Zx),
                GCode::PlaneYz => Some(Plane::Yz),
                _ => None,
            })
            .unwrap_or(state.plane);
        if plane != Plane::Xy && block_has_arc_motion(block, state) {
            return Err(unsupported(
                line,
                "G18/G19 arc",
                format!(
                    "{} arcs are not supported; rotation is defined for G17 XY arcs only",
                    plane.g_code()
                ),
            ));
        }
        Ok(())
    }

    /// Rejects a planned motion that rotation cannot represent.
    ///
    /// This runs for every angle, including `0.0`, so an identity transform
    /// applies the same rules as a real rotation.
    fn check_plan_supported(
        &self,
        line: usize,
        plan: &Plan,
        start: [f64; 3],
    ) -> Result<(), RotationError> {
        if plan.moves.is_empty() {
            return Ok(());
        }
        if plan.state.distance != DistanceMode::Absolute {
            return Err(unsupported(
                line,
                "G91",
                "incremental motion is not supported; rotate an absolute (G90) program",
            ));
        }
        match plan.state.motion {
            MotionMode::Rapid | MotionMode::Linear => {
                if !matches!(plan.moves.as_slice(), [PlannedMove::Line { .. }]) {
                    return Err(unsupported(
                        line,
                        "motion",
                        "the block produced more than one straight move",
                    ));
                }
            }
            MotionMode::ArcCw | MotionMode::ArcCcw => {
                let [PlannedMove::Arc { to_work, plane, .. }] = plan.moves.as_slice() else {
                    return Err(unsupported(
                        line,
                        "motion",
                        "the block produced more than one arc",
                    ));
                };
                if *plane != Plane::Xy {
                    return Err(unsupported(
                        line,
                        "G18/G19 arc",
                        format!(
                            "{} arcs are not supported; rotation is defined for G17 XY arcs only",
                            plane.g_code()
                        ),
                    ));
                }
                if (to_work[2] - start[2]).abs() > EPSILON_MM {
                    return Err(unsupported(
                        line,
                        "helical arc",
                        "helical input arcs are not supported; only planar G17 arcs can be rotated",
                    ));
                }
            }
            MotionMode::Probe(_) => {
                return Err(unsupported(line, "G38.x", "probing moves are not rotated"));
            }
            MotionMode::Canned(_) => {
                return Err(unsupported(
                    line,
                    "canned cycle",
                    "canned cycles cannot be rotated; expand the cycle first",
                ));
            }
            MotionMode::None => {}
        }
        Ok(())
    }

    /// Rewrites one planned block, or copies it when it does not change XY.
    fn rewrite_block(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        plan: &Plan,
        start: [f64; 3],
    ) -> Result<(), RotationError> {
        if plan.moves.is_empty() {
            return self.copy_source(parsed);
        }
        match plan.state.motion {
            MotionMode::Rapid | MotionMode::Linear => {
                let [PlannedMove::Line { to_work, .. }] = plan.moves.as_slice() else {
                    return Err(RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the validated straight move has an unexpected shape".to_string(),
                    });
                };
                if block.x.is_none() && block.y.is_none() {
                    // A Z-only move has no XY endpoint to rotate.
                    return self.copy_source(parsed);
                }
                let units = plan.state.units;
                self.rotate_line(parsed, block, units, *to_work)
            }
            MotionMode::ArcCw | MotionMode::ArcCcw => {
                let [
                    PlannedMove::Arc {
                        to_work,
                        center_work,
                        ..
                    },
                ] = plan.moves.as_slice()
                else {
                    return Err(RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the validated arc has an unexpected shape".to_string(),
                    });
                };
                let units = plan.state.units;
                self.rotate_arc(parsed, block, units, start, *to_work, *center_work)
            }
            MotionMode::Probe(_) | MotionMode::Canned(_) => Err(RotationError::OutputInvalid {
                line: parsed.line,
                message: "an unsupported motion reached the rewrite stage".to_string(),
            }),
            MotionMode::None => self.copy_source(parsed),
        }
    }

    /// Rewrites one straight move's absolute XY endpoint.
    fn rotate_line(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        units: Units,
        to: [f64; 3],
    ) -> Result<(), RotationError> {
        if block.x.is_none() && block.y.is_none() {
            return Err(RotationError::OutputInvalid {
                line: parsed.line,
                message: "an XY move has no X or Y word to rotate".to_string(),
            });
        }
        let (x_mm, y_mm) = self.rotate_point(to[0], to[1]);
        self.check_coordinate(parsed.line, x_mm, y_mm)?;
        let x_text = format_units(units, x_mm);
        let y_text = format_units(units, y_mm);

        let mut edits = Vec::with_capacity(2);
        let mut anchor = None;
        if let Some(word) = block.x {
            let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                RotationError::OutputInvalid {
                    line: parsed.line,
                    message: "the X word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(span.0, span.1, x_text.clone()));
            anchor = Some(span.1);
        }
        if let Some(word) = block.y {
            let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                RotationError::OutputInvalid {
                    line: parsed.line,
                    message: "the Y word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(span.0, span.1, y_text.clone()));
            anchor = Some(span.1);
        }
        let anchor = anchor.ok_or_else(|| RotationError::OutputInvalid {
            line: parsed.line,
            message: "the move has no X/Y anchor".to_string(),
        })?;
        if block.x.is_none() {
            edits.push(Edit::insert(anchor, format!(" X{x_text}")));
        }
        if block.y.is_none() {
            edits.push(Edit::insert(anchor, format!(" Y{y_text}")));
        }

        let text =
            apply_edits(&parsed.source, edits).ok_or_else(|| RotationError::OutputInvalid {
                line: parsed.line,
                message: "the rewritten straight move could not be assembled".to_string(),
            })?;
        self.push_line(text)?;
        self.stats.rotated_moves += 1;
        Ok(())
    }

    /// Rewrites one planar arc's endpoint and incremental center offsets.
    fn rotate_arc(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        units: Units,
        start: [f64; 3],
        to: [f64; 3],
        center: [f64; 3],
    ) -> Result<(), RotationError> {
        let (end_x_mm, end_y_mm) = self.rotate_point(to[0], to[1]);
        let (center_x_mm, center_y_mm) = self.rotate_point(center[0], center[1]);
        self.check_coordinate(parsed.line, end_x_mm, end_y_mm)?;
        self.check_coordinate(parsed.line, center_x_mm, center_y_mm)?;
        let (start_x_mm, start_y_mm) = self.rotate_point(start[0], start[1]);
        let i_mm = center_x_mm - start_x_mm;
        let j_mm = center_y_mm - start_y_mm;
        if !i_mm.is_finite() || !j_mm.is_finite() {
            return Err(RotationError::OutputInvalid {
                line: parsed.line,
                message: "the rotated arc center is not finite".to_string(),
            });
        }
        let i_text = format_units(units, i_mm);
        let j_text = format_units(units, j_mm);

        let mut edits = Vec::with_capacity(4);
        // An explicit endpoint is rotated, so both words must be emitted: the
        // axis that was omitted has moved too. A full circle with no X/Y words
        // keeps its implicit (already rotated) start point.
        if block.x.is_some() || block.y.is_some() {
            let mut anchor = None;
            if let Some(word) = block.x {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the arc X word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, format_units(units, end_x_mm)));
                anchor = Some(span.1);
            }
            if let Some(word) = block.y {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the arc Y word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, format_units(units, end_y_mm)));
                anchor = Some(span.1);
            }
            let anchor = anchor.ok_or_else(|| RotationError::OutputInvalid {
                line: parsed.line,
                message: "the arc endpoint has no X/Y anchor".to_string(),
            })?;
            if block.x.is_none() {
                edits.push(Edit::insert(
                    anchor,
                    format!(" X{}", format_units(units, end_x_mm)),
                ));
            }
            if block.y.is_none() {
                edits.push(Edit::insert(
                    anchor,
                    format!(" Y{}", format_units(units, end_y_mm)),
                ));
            }
        }

        if let Some(r_word) = block.r {
            let span = word_span(&parsed.source, r_word.column).ok_or_else(|| {
                RotationError::OutputInvalid {
                    line: parsed.line,
                    message: "the R word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(
                span.0,
                span.1,
                format!("I{i_text} J{j_text}"),
            ));
            self.warnings.push(RotationWarning {
                line: parsed.line,
                kind: RotationWarningKind::RadiusArcResolved,
                message: "the R arc was re-emitted with I/J center offsets; the radius form \
                          is not preserved"
                    .to_string(),
            });
        } else {
            let mut anchor = None;
            if let Some(word) = block.i {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the I word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, i_text.clone()));
                anchor = Some(span.1);
            }
            if let Some(word) = block.j {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    RotationError::OutputInvalid {
                        line: parsed.line,
                        message: "the J word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, j_text.clone()));
                anchor = Some(span.1);
            }
            let anchor = anchor.ok_or_else(|| RotationError::OutputInvalid {
                line: parsed.line,
                message: "the arc has neither I/J nor R center words".to_string(),
            })?;
            if block.i.is_none() {
                edits.push(Edit::insert(anchor, format!(" I{i_text}")));
            }
            if block.j.is_none() {
                edits.push(Edit::insert(anchor, format!(" J{j_text}")));
            }
        }

        let text =
            apply_edits(&parsed.source, edits).ok_or_else(|| RotationError::OutputInvalid {
                line: parsed.line,
                message: "the rewritten arc could not be assembled".to_string(),
            })?;
        self.push_line(text)?;
        self.stats.rotated_arcs += 1;
        Ok(())
    }

    /// Rotates one absolute XY point about the pivot, returning millimetres.
    fn rotate_point(&self, x_mm: f64, y_mm: f64) -> (f64, f64) {
        let dx = x_mm - self.options.pivot_mm.0;
        let dy = y_mm - self.options.pivot_mm.1;
        (
            self.options.pivot_mm.0 + dx * self.cos - dy * self.sin,
            self.options.pivot_mm.1 + dx * self.sin + dy * self.cos,
        )
    }

    /// Rejects a rotated point that the emitted program could not reparse.
    fn check_coordinate(&self, line: usize, x_mm: f64, y_mm: f64) -> Result<(), RotationError> {
        if !x_mm.is_finite()
            || !y_mm.is_finite()
            || x_mm.abs() > MAX_COORDINATE_MM
            || y_mm.abs() > MAX_COORDINATE_MM
        {
            return Err(RotationError::CoordinateOutOfRange { line, x_mm, y_mm });
        }
        Ok(())
    }

    /// Copies a source line verbatim.
    fn copy_source(&mut self, parsed: &ProgramLine) -> Result<(), RotationError> {
        self.push_line(parsed.source.clone())
    }

    /// Pushes one output line or fails at the safety cap.
    fn push_line(&mut self, text: String) -> Result<(), RotationError> {
        if self.output.len() >= MAX_ROTATED_OUTPUT_LINES {
            return Err(RotationError::OutputTooLong {
                limit: MAX_ROTATED_OUTPUT_LINES,
            });
        }
        self.output.push(text);
        Ok(())
    }
}

/// One in-place edit of a source line.
#[derive(Debug, Clone)]
struct Edit {
    /// Byte offset of the first replaced byte, or the insertion point.
    start: usize,
    /// Byte offset one past the last replaced byte; equals `start` to insert.
    end: usize,
    /// Replacement text.
    text: String,
}

impl Edit {
    /// Creates a replacement for a byte range.
    fn replace(start: usize, end: usize, text: String) -> Self {
        Self { start, end, text }
    }

    /// Creates an insertion at a byte offset.
    fn insert(at: usize, text: String) -> Self {
        Self {
            start: at,
            end: at,
            text,
        }
    }
}

/// Applies non-overlapping edits to `source`, preserving everything else.
///
/// Returns `None` when an edit is out of bounds or overlaps an earlier one, so
/// a bad span can never panic or corrupt the line.
fn apply_edits(source: &str, mut edits: Vec<Edit>) -> Option<String> {
    edits.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let mut output = String::with_capacity(source.len() + 32);
    let mut cursor = 0usize;
    for edit in edits {
        if edit.start < cursor || edit.end < edit.start || edit.end > source.len() {
            return None;
        }
        output.push_str(source.get(cursor..edit.start)?);
        output.push_str(&edit.text);
        cursor = edit.end;
    }
    output.push_str(source.get(cursor..)?);
    Some(output)
}

/// Returns the byte range of the numeric token of a word.
///
/// `column` is the 1-based byte column of the letter, as reported by the lexer.
/// The returned range starts after the letter and excludes it.
fn number_span(source: &str, column: usize) -> Option<(usize, usize)> {
    let letter = column.checked_sub(1)?;
    let bytes = source.as_bytes();
    if !bytes.get(letter)?.is_ascii_alphabetic() {
        return None;
    }
    let start = letter + 1;
    let mut end = start;
    while let Some(byte) = bytes.get(end) {
        if matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.') {
            end += 1;
        } else {
            break;
        }
    }
    (end > start).then_some((start, end))
}

/// Returns the byte range of a complete word, including its letter.
fn word_span(source: &str, column: usize) -> Option<(usize, usize)> {
    let (start, end) = number_span(source, column)?;
    Some((start.saturating_sub(1), end))
}

/// Returns `true` when the block commands an arc move.
fn block_has_arc_motion(block: &Block, state: &ParserState) -> bool {
    if block
        .g
        .iter()
        .any(|word| matches!(word.code, GCode::ArcCw | GCode::ArcCcw))
    {
        return true;
    }
    state.motion.is_arc()
        && (block.has_axis_words() || block.has_center_words() || block.r.is_some())
}

/// Builds one unsupported-construct error.
fn unsupported(line: usize, construct: &'static str, message: impl Into<String>) -> RotationError {
    RotationError::UnsupportedConstruct {
        line,
        construct,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_spans_cover_the_whole_token() {
        assert_eq!(number_span("G0 X-1.25", 4), Some((4, 9)));
        assert_eq!(word_span("G0 X-1.25", 4), Some((3, 9)));
        assert_eq!(number_span("g1x10", 3), Some((3, 5)));
        assert_eq!(number_span("G0 X", 4), None);
        assert_eq!(number_span("G0 X1", 2), None);
        assert_eq!(number_span("", 1), None);
        assert_eq!(number_span("G0 X1", 0), None);
    }

    #[test]
    fn edits_preserve_text_outside_their_spans() {
        let mut edits = vec![
            Edit::replace(4, 5, "9".to_string()),
            Edit::insert(5, " Y2".to_string()),
        ];
        assert_eq!(
            apply_edits("G1 X1Z0", edits.clone()).as_deref(),
            Some("G1 X9 Y2Z0")
        );
        edits.push(Edit::replace(8, 7, "bad".to_string()));
        assert_eq!(apply_edits("G1 X1Z0", edits), None);
    }

    #[test]
    fn options_reject_bad_values() {
        let good = RotationOptions {
            angle_deg: 90.0,
            pivot_mm: (1.0, -1.0),
        };
        assert!(good.validate().is_ok());

        for angle_deg in [f64::NAN, f64::INFINITY, 3_600.001, -3_600.001] {
            let options = RotationOptions {
                angle_deg,
                pivot_mm: (0.0, 0.0),
            };
            assert!(
                matches!(
                    options.validate(),
                    Err(RotationError::InvalidOptions { .. })
                ),
                "angle {angle_deg} must be rejected"
            );
        }
        for pivot_mm in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (1.0e8, 0.0)] {
            let options = RotationOptions {
                angle_deg: 0.0,
                pivot_mm,
            };
            assert!(options.validate().is_err(), "pivot {pivot_mm:?}");
        }
    }
}
