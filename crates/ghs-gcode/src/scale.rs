//! Sender-side uniform scaling of a whole program about an XY pivot.
//!
//! This is a pure source-to-source rewrite, like
//! [`apply_rotation`](crate::apply_rotation), not a live stream correction. A
//! caller parses the source once, scales it, and loads the result for preview
//! or a job. The original program is never mutated, so the app can keep it for
//! Revert.
//!
//! # Algorithm
//!
//! Every explicit absolute XY endpoint of a `G0`/`G1` move and every `G17`
//! `G2`/`G3` arc is scaled about [`ScaleOptions::pivot_mm`]:
//!
//! ```text
//! scaled = pivot + factor * (point - pivot)
//! ```
//!
//! `I`/`J` center offsets are incremental vectors from the arc start, so they
//! are multiplied by [`ScaleOptions::factor`] directly. Uniform scaling maps a
//! circle to a circle: the arc radius and center scale with its endpoints, and
//! the sweep and direction are unchanged. An arc described with `R` is resolved
//! to its center and re-emitted with `I`/`J`, with a
//! [`ScaleWarningKind::RadiusArcResolved`] warning.
//!
//! The pivot is in the same **program (work)** frame as the source endpoints;
//! work offsets configured on the controller are not applied. All geometry is
//! converted to millimetres, scaled there, and emitted in the active
//! `G20`/`G21` unit mode. A program in inches therefore stays in inches; it is
//! never silently converted. Coordinates use up to six decimal places in
//! program units with trailing zeros trimmed, matching the height-map
//! formatter.
//!
//! A block with no explicit X or Y word (a `Z`-only move, `F`-only line,
//! comment, blank line, tool change, or modal line) is copied byte for byte. A
//! motion block is rewritten in place: its `N` word, modal words, `Z`, `F`,
//! `S`, `T`, comments, spacing, lowercase letters, and block-delete marker are
//! left untouched. An absolute XY endpoint is scaled even when the source
//! block happens to be a non-move, so a return to the programmed origin lands
//! on the scaled origin. Because no line is split, `N` line numbers stay
//! valid.
//!
//! At exactly `1.0` the validated source is returned unchanged.
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
//! The following return [`ScaleError::UnsupportedConstruct`]; a rejected
//! program produces no output and the caller's source is not touched:
//!
//! - `G91` incremental motion: a source scale cannot tell an absolute endpoint
//!   from an incremental vector without a full frame model.
//! - `G53` machine-coordinate motion and `G28`/`G30` home moves.
//! - Probing (`G38.x`) and canned cycles.
//! - `G10`, `G92`/`G92.1`-`G92.3`, and `G51` scaling: a source scale cannot
//!   reproduce a mid-program frame change.
//! - `G18`/`G19` arcs. Straight moves in those planes are fine.
//! - Helical input arcs (a Z change across a `G17` arc).
//! - `G93` inverse-time feed.
//! - Lines carrying a `*` checksum: rewriting invalidates the checksum.
//! - A mid-program switch to a different work coordinate system.
//!
//! Parenthesised expressions, flow control, `G41`/`G42`, and the other codes
//! this crate already rejects fail during the initial parse with
//! [`ScaleError::Parse`].
//!
//! # Starting point
//!
//! As with rotation and height-map compensation, the controller's current
//! point is not modelled. A program should position XY explicitly before its
//! first cut; a program whose first motion is a `Z`-only or full-circle block
//! inherits the machine's existing XY position. Scaling the source cannot move
//! the tool to the scaled origin.
//!
//! # Limits
//!
//! The factor must be finite and within
//! [`MIN_SCALE_FACTOR`]..=[`MAX_SCALE_FACTOR`]. A negative factor would mirror
//! the path (and reverse arc direction) and zero would collapse it, so both are
//! rejected. The pivot must be finite and within
//! +/-[`MAX_COORDINATE_MM`](crate::MAX_COORDINATE_MM). A transformed program
//! may contain at most [`MAX_SCALED_OUTPUT_LINES`] lines; because no line is
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
use crate::rewrite::{Edit, apply_edits, number_span, word_span};
use crate::state::{DistanceMode, MotionMode, ParserState, Plane};

/// Smallest accepted uniform scale factor.
///
/// A smaller factor would turn ordinary features into geometry below the
/// parser's arc-radius and output resolution.
pub const MIN_SCALE_FACTOR: f64 = 1.0e-6;

/// Largest accepted uniform scale factor.
///
/// This keeps every scaled coordinate within the parser's range for any
/// realistic source and the arithmetic far away from overflow.
pub const MAX_SCALE_FACTOR: f64 = 1.0e6;

/// Safety cap on output lines.
///
/// Scaling never splits a line, so this also bounds the input program. The cap
/// matches [`MAX_ROTATED_OUTPUT_LINES`](crate::MAX_ROTATED_OUTPUT_LINES).
pub const MAX_SCALED_OUTPUT_LINES: usize = 200_000;

/// Two XY points closer than this are the same point for motion detection.
const EPSILON_MM: f64 = 1.0e-9;

/// Inputs for [`apply_scale`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleOptions {
    /// Uniform scale factor applied to both XY axes.
    ///
    /// The value must be finite and within
    /// [`MIN_SCALE_FACTOR`]..=[`MAX_SCALE_FACTOR`]. `1.0` is an identity
    /// transform. Negative and zero factors are rejected.
    pub factor: f64,
    /// Pivot point in the XY plane, in millimetres.
    ///
    /// The pivot is in the same **program (work)** frame as the source
    /// endpoints; controller work offsets are not applied. It is always in
    /// millimetres, even for a `G20` program.
    pub pivot_mm: (f64, f64),
}

impl ScaleOptions {
    /// Validates the factor and the pivot.
    ///
    /// # Errors
    ///
    /// Returns [`ScaleError::InvalidOptions`] for a non-finite factor, a factor
    /// outside [`MIN_SCALE_FACTOR`]..=[`MAX_SCALE_FACTOR`], or a pivot beyond
    /// +/-[`MAX_COORDINATE_MM`](crate::MAX_COORDINATE_MM).
    pub fn validate(&self) -> Result<(), ScaleError> {
        if !self.factor.is_finite() || !(MIN_SCALE_FACTOR..=MAX_SCALE_FACTOR).contains(&self.factor)
        {
            return Err(ScaleError::InvalidOptions {
                message: format!(
                    "factor {} must be finite and between {MIN_SCALE_FACTOR} and \
                     {MAX_SCALE_FACTOR}",
                    self.factor
                ),
            });
        }
        for (axis, value) in ["X", "Y"].iter().zip([self.pivot_mm.0, self.pivot_mm.1]) {
            if !value.is_finite() || value.abs() > MAX_COORDINATE_MM {
                return Err(ScaleError::InvalidOptions {
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

/// Aggregate counters for one scale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScaleStats {
    /// Source lines, including blank and comment-only lines.
    pub input_lines: usize,
    /// Emitted lines. Scaling never splits or drops a line, so this equals
    /// `input_lines` for a successful non-identity transform.
    pub output_lines: usize,
    /// `G0`/`G1` blocks whose explicit XY endpoint was rewritten.
    ///
    /// A block with no `X` or `Y` word (a `Z`-only move) and a modal-only or
    /// comment line is copied and not counted. A non-moving absolute endpoint
    /// with an `X`/`Y` word is still rewritten.
    pub scaled_moves: usize,
    /// `G2`/`G3` blocks whose endpoint or center was scaled.
    pub scaled_arcs: usize,
}

/// A completed scale rewrite.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaledProgram {
    /// Rewritten program text. Lines end with `\n`.
    pub source: String,
    /// Counters describing what was rewritten.
    pub stats: ScaleStats,
    /// Non-fatal issues, currently an `R` arc resolved to `I`/`J`.
    pub warnings: Vec<ScaleWarning>,
}

/// Classification of a [`ScaleWarning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleWarningKind {
    /// An `R`-form arc was re-emitted with `I`/`J` center offsets.
    RadiusArcResolved,
}

/// A non-fatal issue found while scaling the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleWarning {
    /// 1-based source line, or `0` for a program-wide warning.
    pub line: usize,
    /// Machine-readable warning class.
    pub kind: ScaleWarningKind,
    /// Human-readable explanation.
    pub message: String,
}

/// A fatal scale error.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ScaleError {
    /// An option is non-finite or outside its supported range.
    #[error("invalid scale options: {message}")]
    InvalidOptions {
        /// Explanation naming the bad field and value.
        message: String,
    },
    /// The input program is not valid G-code.
    #[error(
        "the program could not be parsed ({} error(s)); fix the program before scaling it",
        errors.len()
    )]
    Parse {
        /// All parser diagnostics, sorted by line and column.
        errors: Vec<ParseError>,
    },
    /// The program uses a construct that scaling cannot represent safely.
    #[error("line {line}: {message}")]
    UnsupportedConstruct {
        /// 1-based source line.
        line: usize,
        /// Short stable tag for the construct, for example `"G91"`.
        construct: &'static str,
        /// Explanation for the operator.
        message: String,
    },
    /// A scaled XY coordinate would be outside the parser's range.
    #[error("line {line}: scaled XY ({x_mm}, {y_mm}) mm is outside +/-{MAX_COORDINATE_MM} mm")]
    CoordinateOutOfRange {
        /// 1-based source line.
        line: usize,
        /// Scaled X, in millimetres.
        x_mm: f64,
        /// Scaled Y, in millimetres.
        y_mm: f64,
    },
    /// The rewritten program would exceed the output-line safety cap.
    #[error("the scaled program would exceed {limit} output lines")]
    OutputTooLong {
        /// The cap, [`MAX_SCALED_OUTPUT_LINES`].
        limit: usize,
    },
    /// The rewritten line could not be built, or the output did not reparse.
    ///
    /// This is a safety net for a transform bug and never describes bad input;
    /// a bad source returns [`Self::Parse`] instead.
    #[error("line {line}: the scaled program did not verify: {message}")]
    OutputInvalid {
        /// 1-based source line, or `0` when no location is known.
        line: usize,
        /// Explanation.
        message: String,
    },
}

impl ScaleError {
    /// Returns the parser diagnostics when this is a [`Self::Parse`] error.
    #[must_use]
    pub fn parse_errors(&self) -> Option<&[ParseError]> {
        match self {
            Self::Parse { errors } => Some(errors),
            _ => None,
        }
    }
}

/// Scales a whole program uniformly about an XY pivot.
///
/// The input is fully parsed first; a malformed program returns
/// [`ScaleError::Parse`] and never panics. See the module documentation for the
/// transform rules, preserved constructs, and the rejection list.
///
/// # Errors
///
/// Returns a typed error for invalid options, parse failures, unsupported
/// constructs, a scaled coordinate outside the parser's range, an output that
/// exceeds the safety cap, or an output that fails to reparse.
pub fn apply_scale(source: &str, options: &ScaleOptions) -> Result<ScaledProgram, ScaleError> {
    options.validate()?;
    let program = parse_program(source).map_err(|errors| ScaleError::Parse { errors })?;
    if program.lines.len() > MAX_SCALED_OUTPUT_LINES {
        return Err(ScaleError::OutputTooLong {
            limit: MAX_SCALED_OUTPUT_LINES,
        });
    }

    let identity = options.factor == 1.0;
    let mut transform = Transform {
        options,
        identity,
        output: Vec::with_capacity(program.lines.len()),
        stats: ScaleStats {
            input_lines: program.lines.len(),
            ..ScaleStats::default()
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
        return Err(ScaleError::OutputInvalid { line, message });
    }
    Ok(ScaledProgram {
        source: output,
        stats: transform.stats,
        warnings: transform.warnings,
    })
}

/// Mutable state of one rewrite.
struct Transform<'a> {
    options: &'a ScaleOptions,
    identity: bool,
    output: Vec<String>,
    stats: ScaleStats,
    warnings: Vec<ScaleWarning>,
    wcs: Option<Wcs>,
}

impl Transform<'_> {
    /// Validates, plans, and rewrites one source line.
    fn process_line(
        &mut self,
        parsed: &ProgramLine,
        parser: &mut Parser,
    ) -> Result<(), ScaleError> {
        if parsed.checksum.is_some() {
            return Err(unsupported(
                parsed.line,
                "checksum",
                "checksummed input is not supported by scaling; remove the * checksums first",
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
            Block::parse(parsed.line, &lexed).map_err(|errors| ScaleError::Parse { errors })?;
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
                .map_err(|errors| ScaleError::Parse { errors })?
        };
        self.check_plan_supported(parsed.line, &plan, start)?;

        if self.identity {
            self.copy_source(parsed)?;
        } else {
            self.rewrite_block(parsed, &block, &plan, start)?;
        }

        parser
            .parse_lexed(parsed.line, &parsed.source, lexed)
            .map_err(|errors| ScaleError::Parse { errors })?;
        Ok(())
    }

    /// Rejects constructs whose coordinates or frame scaling cannot model.
    fn check_supported(
        &mut self,
        line: usize,
        state: &ParserState,
        block: &Block,
    ) -> Result<(), ScaleError> {
        for word in &block.g {
            match word.code {
                GCode::Incremental => {
                    return Err(unsupported(
                        line,
                        "G91",
                        "G91 incremental motion is not supported; scale an absolute (G90) program",
                    ));
                }
                GCode::MachineCoords => {
                    return Err(unsupported(
                        line,
                        "G53",
                        "G53 machine-coordinate motion cannot be scaled",
                    ));
                }
                GCode::Home | GCode::SetHome | GCode::SecondHome | GCode::SetSecondHome => {
                    return Err(unsupported(
                        line,
                        "G28/G30",
                        "G28/G30 home motion targets machine coordinates and cannot be scaled",
                    ));
                }
                GCode::Probe(_) => {
                    return Err(unsupported(
                        line,
                        "G38.x",
                        "probing moves are not scaled; scale the part program, not a probe cycle",
                    ));
                }
                GCode::Canned(_) => {
                    return Err(unsupported(
                        line,
                        "canned cycle",
                        "canned cycles cannot be scaled; expand the cycle first",
                    ));
                }
                GCode::SetCoordinateSystem => {
                    return Err(unsupported(
                        line,
                        "G10",
                        "G10 work-offset changes are not supported by a source scale",
                    ));
                }
                GCode::SetOffsets
                | GCode::ClearOffsets
                | GCode::SuspendOffsets
                | GCode::RestoreOffsets => {
                    return Err(unsupported(
                        line,
                        "G92",
                        "G92 offset changes are not supported by a source scale",
                    ));
                }
                GCode::FeedInverseTime => {
                    return Err(unsupported(
                        line,
                        "G93",
                        "G93 inverse-time feed is not supported by scaling",
                    ));
                }
                GCode::ScalingSet => {
                    return Err(unsupported(
                        line,
                        "G51",
                        "G51 scaling changes the XY mapping and cannot be combined with a source scale",
                    ));
                }
                GCode::Wcs(wcs) => match self.wcs {
                    None => self.wcs = Some(wcs),
                    Some(current) if current != wcs => {
                        return Err(unsupported(
                            line,
                            "work coordinate system",
                            format!(
                                "the program switches from {} to {}; scaling supports one \
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
                    "{} arcs are not supported; scaling is defined for G17 XY arcs only",
                    plane.g_code()
                ),
            ));
        }
        Ok(())
    }

    /// Rejects a planned motion that scaling cannot represent.
    ///
    /// This runs for every factor, including `1.0`, so an identity transform
    /// applies the same rules as a real scale.
    fn check_plan_supported(
        &self,
        line: usize,
        plan: &Plan,
        start: [f64; 3],
    ) -> Result<(), ScaleError> {
        if plan.moves.is_empty() {
            return Ok(());
        }
        if plan.state.distance != DistanceMode::Absolute {
            return Err(unsupported(
                line,
                "G91",
                "incremental motion is not supported; scale an absolute (G90) program",
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
                            "{} arcs are not supported; scaling is defined for G17 XY arcs only",
                            plane.g_code()
                        ),
                    ));
                }
                if (to_work[2] - start[2]).abs() > EPSILON_MM {
                    return Err(unsupported(
                        line,
                        "helical arc",
                        "helical input arcs are not supported; only planar G17 arcs can be scaled",
                    ));
                }
            }
            MotionMode::Probe(_) => {
                return Err(unsupported(line, "G38.x", "probing moves are not scaled"));
            }
            MotionMode::Canned(_) => {
                return Err(unsupported(
                    line,
                    "canned cycle",
                    "canned cycles cannot be scaled; expand the cycle first",
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
    ) -> Result<(), ScaleError> {
        if plan.moves.is_empty() {
            return self.copy_source(parsed);
        }
        match plan.state.motion {
            MotionMode::Rapid | MotionMode::Linear => {
                let [PlannedMove::Line { to_work, .. }] = plan.moves.as_slice() else {
                    return Err(ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the validated straight move has an unexpected shape".to_string(),
                    });
                };
                if block.x.is_none() && block.y.is_none() {
                    // A Z-only move has no XY endpoint to scale.
                    return self.copy_source(parsed);
                }
                let units = plan.state.units;
                self.scale_line(parsed, block, units, *to_work)
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
                    return Err(ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the validated arc has an unexpected shape".to_string(),
                    });
                };
                let units = plan.state.units;
                self.scale_arc(parsed, block, units, start, *to_work, *center_work)
            }
            MotionMode::Probe(_) | MotionMode::Canned(_) => Err(ScaleError::OutputInvalid {
                line: parsed.line,
                message: "an unsupported motion reached the rewrite stage".to_string(),
            }),
            MotionMode::None => self.copy_source(parsed),
        }
    }

    /// Rewrites one straight move's absolute XY endpoint.
    fn scale_line(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        units: Units,
        to: [f64; 3],
    ) -> Result<(), ScaleError> {
        if block.x.is_none() && block.y.is_none() {
            return Err(ScaleError::OutputInvalid {
                line: parsed.line,
                message: "an XY move has no X or Y word to scale".to_string(),
            });
        }
        let (x_mm, y_mm) = self.scale_point(to[0], to[1]);
        self.check_coordinate(parsed.line, x_mm, y_mm)?;
        let x_text = format_units(units, x_mm);
        let y_text = format_units(units, y_mm);

        let mut edits = Vec::with_capacity(2);
        let mut anchor = None;
        if let Some(word) = block.x {
            let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                ScaleError::OutputInvalid {
                    line: parsed.line,
                    message: "the X word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(span.0, span.1, x_text.clone()));
            anchor = Some(span.1);
        }
        if let Some(word) = block.y {
            let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                ScaleError::OutputInvalid {
                    line: parsed.line,
                    message: "the Y word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(span.0, span.1, y_text.clone()));
            anchor = Some(span.1);
        }
        let anchor = anchor.ok_or_else(|| ScaleError::OutputInvalid {
            line: parsed.line,
            message: "the move has no X/Y anchor".to_string(),
        })?;
        if block.x.is_none() {
            edits.push(Edit::insert(anchor, format!(" X{x_text}")));
        }
        if block.y.is_none() {
            edits.push(Edit::insert(anchor, format!(" Y{y_text}")));
        }

        let text = apply_edits(&parsed.source, edits).ok_or_else(|| ScaleError::OutputInvalid {
            line: parsed.line,
            message: "the rewritten straight move could not be assembled".to_string(),
        })?;
        self.push_line(text)?;
        self.stats.scaled_moves += 1;
        Ok(())
    }

    /// Rewrites one planar arc's endpoint and incremental center offsets.
    fn scale_arc(
        &mut self,
        parsed: &ProgramLine,
        block: &Block,
        units: Units,
        start: [f64; 3],
        to: [f64; 3],
        center: [f64; 3],
    ) -> Result<(), ScaleError> {
        let (end_x_mm, end_y_mm) = self.scale_point(to[0], to[1]);
        let (center_x_mm, center_y_mm) = self.scale_point(center[0], center[1]);
        self.check_coordinate(parsed.line, end_x_mm, end_y_mm)?;
        self.check_coordinate(parsed.line, center_x_mm, center_y_mm)?;
        let (start_x_mm, start_y_mm) = self.scale_point(start[0], start[1]);
        let i_mm = center_x_mm - start_x_mm;
        let j_mm = center_y_mm - start_y_mm;
        if !i_mm.is_finite() || !j_mm.is_finite() {
            return Err(ScaleError::OutputInvalid {
                line: parsed.line,
                message: "the scaled arc center is not finite".to_string(),
            });
        }
        let i_text = format_units(units, i_mm);
        let j_text = format_units(units, j_mm);

        let mut edits = Vec::with_capacity(4);
        // An explicit endpoint is scaled, so both words must be emitted: the
        // axis that was omitted has moved too. A full circle with no X/Y words
        // keeps its implicit (already scaled) start point.
        if block.x.is_some() || block.y.is_some() {
            let mut anchor = None;
            if let Some(word) = block.x {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the arc X word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, format_units(units, end_x_mm)));
                anchor = Some(span.1);
            }
            if let Some(word) = block.y {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the arc Y word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, format_units(units, end_y_mm)));
                anchor = Some(span.1);
            }
            let anchor = anchor.ok_or_else(|| ScaleError::OutputInvalid {
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
                ScaleError::OutputInvalid {
                    line: parsed.line,
                    message: "the R word has no numeric token to replace".to_string(),
                }
            })?;
            edits.push(Edit::replace(
                span.0,
                span.1,
                format!("I{i_text} J{j_text}"),
            ));
            self.warnings.push(ScaleWarning {
                line: parsed.line,
                kind: ScaleWarningKind::RadiusArcResolved,
                message: "the R arc was re-emitted with I/J center offsets; the radius form \
                          is not preserved"
                    .to_string(),
            });
        } else {
            let mut anchor = None;
            if let Some(word) = block.i {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the I word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, i_text.clone()));
                anchor = Some(span.1);
            }
            if let Some(word) = block.j {
                let span = number_span(&parsed.source, word.column).ok_or_else(|| {
                    ScaleError::OutputInvalid {
                        line: parsed.line,
                        message: "the J word has no numeric token to replace".to_string(),
                    }
                })?;
                edits.push(Edit::replace(span.0, span.1, j_text.clone()));
                anchor = Some(span.1);
            }
            let anchor = anchor.ok_or_else(|| ScaleError::OutputInvalid {
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

        let text = apply_edits(&parsed.source, edits).ok_or_else(|| ScaleError::OutputInvalid {
            line: parsed.line,
            message: "the rewritten arc could not be assembled".to_string(),
        })?;
        self.push_line(text)?;
        self.stats.scaled_arcs += 1;
        Ok(())
    }

    /// Scales one absolute XY point about the pivot, returning millimetres.
    fn scale_point(&self, x_mm: f64, y_mm: f64) -> (f64, f64) {
        let factor = self.options.factor;
        (
            self.options.pivot_mm.0 + (x_mm - self.options.pivot_mm.0) * factor,
            self.options.pivot_mm.1 + (y_mm - self.options.pivot_mm.1) * factor,
        )
    }

    /// Rejects a scaled point that the emitted program could not reparse.
    fn check_coordinate(&self, line: usize, x_mm: f64, y_mm: f64) -> Result<(), ScaleError> {
        if !x_mm.is_finite()
            || !y_mm.is_finite()
            || x_mm.abs() > MAX_COORDINATE_MM
            || y_mm.abs() > MAX_COORDINATE_MM
        {
            return Err(ScaleError::CoordinateOutOfRange { line, x_mm, y_mm });
        }
        Ok(())
    }

    /// Copies a source line verbatim.
    fn copy_source(&mut self, parsed: &ProgramLine) -> Result<(), ScaleError> {
        self.push_line(parsed.source.clone())
    }

    /// Pushes one output line or fails at the safety cap.
    fn push_line(&mut self, text: String) -> Result<(), ScaleError> {
        if self.output.len() >= MAX_SCALED_OUTPUT_LINES {
            return Err(ScaleError::OutputTooLong {
                limit: MAX_SCALED_OUTPUT_LINES,
            });
        }
        self.output.push(text);
        Ok(())
    }
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
fn unsupported(line: usize, construct: &'static str, message: impl Into<String>) -> ScaleError {
    ScaleError::UnsupportedConstruct {
        line,
        construct,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_reject_bad_values() {
        let good = ScaleOptions {
            factor: 2.0,
            pivot_mm: (1.0, -1.0),
        };
        assert!(good.validate().is_ok());

        for factor in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            MIN_SCALE_FACTOR / 2.0,
            MAX_SCALE_FACTOR * 2.0,
        ] {
            let options = ScaleOptions {
                factor,
                pivot_mm: (0.0, 0.0),
            };
            assert!(
                matches!(options.validate(), Err(ScaleError::InvalidOptions { .. })),
                "factor {factor} must be rejected"
            );
        }
        for pivot_mm in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (1.0e8, 0.0)] {
            let options = ScaleOptions {
                factor: 2.0,
                pivot_mm,
            };
            assert!(options.validate().is_err(), "pivot {pivot_mm:?}");
        }
    }

    #[test]
    fn scale_point_preserves_the_pivot_and_multiplies_the_offset() {
        let transform = Transform {
            options: &ScaleOptions {
                factor: 2.0,
                pivot_mm: (5.0, 5.0),
            },
            identity: false,
            output: Vec::new(),
            stats: ScaleStats::default(),
            warnings: Vec::new(),
            wcs: None,
        };
        assert_eq!(transform.scale_point(5.0, 5.0), (5.0, 5.0));
        assert_eq!(transform.scale_point(10.0, 0.0), (15.0, -5.0));
    }
}
