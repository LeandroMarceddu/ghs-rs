//! G-code lexer, parser, parser-state emulation, and toolpath generation for
//! the grblHAL dialect.
//!
//! The crate is independent of the UI, transport, and controller session. It
//! turns program text into validated lines, a modal parser state, and display
//! geometry. Safety-critical command gating stays in `ghs-engine`.
//!
//! # Pipeline
//!
//! ```text
//! source line -> lexer (parameters + expressions) -> Block
//!             -> Planner (validate + plan) -> Parser::execute
//!             (modal state + Toolpath emission)
//! ```
//!
//! A block is validated in full before it is applied, so an error never leaves
//! the modal state or the toolpath half-updated.
//!
//! # Example
//!
//! ```
//! use ghs_gcode::parse_program;
//!
//! let program = parse_program(
//!     "G21 G90\n\
//!      G0 X0 Y0 Z5\n\
//!      G1 Z-1 F100\n\
//!      G1 X10\n",
//! )
//! .expect("valid program");
//!
//! assert_eq!(program.stats.rapid_segments, 1);
//! assert_eq!(program.stats.plunge_segments, 1);
//! assert_eq!(program.stats.feed_segments, 1);
//! assert_eq!(program.bounds.min().z, -1.0);
//! assert_eq!(program.bounds.max().x, 10.0);
//! assert_eq!(program.toolpath.segments()[0].kind.label(), "rapid");
//! ```
//!
//! # Supported commands
//!
//! `G0`, `G1`, `G2`, `G3`, `G4`, `G10 L2/L20`, `G17`-`G19`, `G20`/`G21`,
//! `G28`/`G28.1`, `G30`/`G30.1`, `G38.2`-`G38.5`, `G40` (no-op), `G43.1`,
//! `G49`, `G50`/`G51` (Fanuc-style `P` scale and `XYZ` pivot), `G53`-`G59.3`,
//! `G61`/`G64`, `G73`, `G80`/`G81`/`G82`/`G83`/`G85`/`G86`/`G89`,
//! `G90`/`G91`, `G92`/`G92.1`/`G92.2`/`G92.3`, `G93`/`G94`, `G98`/`G99`;
//! `M0`-`M9`, `M30`; `N`, `T`, `S`, `F`; `( )` and `;` comments; `/` block
//! delete; `*` checksums; and a lone `%` marker.
//!
//! # Parameters and expressions
//!
//! Numbered parameters `#1` through `#100` and named parameters `#<name>` are
//! supported. Named parameters are case-insensitive, ASCII, at most 32 bytes,
//! and limited to 64 distinct names. An assignment such as `#1=5`,
//! `#<feed>=250`, or `#2=[#1*3]` updates the parser's table and produces no
//! motion. Reading an unassigned numbered parameter or an unknown named
//! parameter is a structured error.
//!
//! A numeric word value may be a parameter or a bracketed arithmetic
//! expression, for example `X[#1+2.5]`, `F[#<feed>*2]`, and `S[#100]`.
//! Expressions support `+`, `-`, `*`, `/`, unary signs, parentheses, nested
//! `[`/`]` groups, decimal literals, and parameter references. They are
//! evaluated with `f64`; division by zero, non-finite results, unresolved
//! parameters, expressions deeper than 32 nesting levels, and expressions
//! longer than 512 bytes are rejected. Expressions are not allowed in `G`,
//! `M`, `N`, or `O` words.
//!
//! An assignment value is deliberately one of:
//!
//! - a signed decimal: `#1=5`, `#1=-2.5`;
//! - a parameter reference: `#1=#2`, `#1=#<feed>`;
//! - a bracketed expression: `#1=[2+3*4]`.
//!
//! Compound arithmetic must be bracketed. `#1=2+3` is a
//! [`ParseErrorKind::MalformedExpression`], not an implicit expression.
//! Because `*` after a value is the line checksum, always put the whole
//! calculation inside brackets. A top-level `(` starts a G-code comment, so
//! assignment grouping also uses `[...]`. Assignments are staged while a block
//! is lexed and committed only after the block validates; a failed block
//! leaves the parameter table untouched.
//!
//! Unsupported codes are rejected with a structured error instead of being
//! silently ignored, so a validation view can explain why a program will not
//! run.
//!
//! # Deferred (rejected, not emulated)
//!
//! Flow control (`IF`/`WHILE`/`GOTO`/`DO`/`ENDIF`/`SUB`/`M98`/`M99`), `O`-word
//! subprograms, cutter radius compensation (`G41`/`G42`), lathe cycles,
//! `G84`/`G87`/`G88` tapping, `G90.1`/`G91.1` absolute arc centers, and extra
//! axes (`A`, `B`, `C`, `U`, `V`, `W`) remain unsupported. Flow control returns
//! [`ParseErrorKind::UnsupportedExpression`]; subprograms and the other forms
//! return [`ParseErrorKind::UnsupportedCode`] or
//! [`ParseErrorKind::UnsupportedWord`].
//!
//! # Height-map compensation
//!
//! [`apply_height_compensation`] is the pure sender-side rewrite behind the
//! explicit **Apply height map** action (PROBE-11). It parses a whole program,
//! splits `G0`/`G1` moves and `G2`/`G3` arcs at a caller-supplied XY step,
//! samples a callback in machine XY, adds the returned Z delta, and leaves
//! non-motion content in place. The original text is not mutated, so the app
//! can keep it for Revert.
//!
//! ```
//! use ghs_gcode::{
//!     HeightCompensationOptions, OutsideMap, apply_height_compensation,
//! };
//!
//! let options = HeightCompensationOptions {
//!     work_offset_xy_mm: [0.0, 0.0],
//!     max_step_mm: 1.0,
//!     outside: OutsideMap::Leave,
//! };
//! let result = apply_height_compensation("G21 G90\nG1 X2 F100\n", &options, |_x, _y| {
//!     Some(0.1)
//! })
//! .expect("transform");
//! assert_eq!(result.stats.compensated_motions, 2);
//! assert!(result.source.contains("Z0.1"));
//! ```
//!
//! # Rotation
//!
//! [`apply_rotation`] is the pure sender-side rewrite behind a **Rotate
//! program** action. It rotates absolute XY endpoints and `I`/`J` center
//! offsets of `G0`/`G1` moves and `G17` `G2`/`G3` arcs about a pivot, resolves
//! `R` arcs to `I`/`J`, and leaves every other line untouched. It works in
//! millimetres and emits in the program's `G20`/`G21` unit mode.
//!
//! ```
//! use ghs_gcode::{RotationOptions, apply_rotation};
//!
//! let options = RotationOptions {
//!     angle_deg: 90.0,
//!     pivot_mm: (0.0, 0.0),
//! };
//! let result = apply_rotation("G21 G90\nG0 X10 Y0\n", &options).expect("rotate");
//! assert!(result.source.contains("G0 X0 Y10"));
//! assert_eq!(result.stats.rotated_moves, 1);
//! ```
//!
//! # Scaling
//!
//! [`apply_scale`] is the pure sender-side rewrite behind a **Scale
//! program** action. It scales absolute XY endpoints and `I`/`J` center
//! offsets of `G0`/`G1` moves and `G17` `G2`/`G3` arcs about a pivot by one
//! uniform positive factor, resolves `R` arcs to `I`/`J`, and leaves every
//! other line untouched. It works in millimetres and emits in the program's
//! `G20`/`G21` unit mode. Z, feed, speed, tool, and modal state are preserved.
//!
//! ```
//! use ghs_gcode::{ScaleOptions, apply_scale};
//!
//! let options = ScaleOptions {
//!     factor: 2.0,
//!     pivot_mm: (0.0, 0.0),
//! };
//! let result = apply_scale("G21 G90\nG0 X10 Y0\n", &options).expect("scale");
//! assert!(result.source.contains("G0 X20 Y0"));
//! assert_eq!(result.stats.scaled_moves, 1);
//! ```
//!
//! # Coordinates
//!
//! Stored coordinates are millimetres. The display toolpath applies `G51`
//! scaling, caller-supplied rotation, the active work offset, `G92`, and the `G43.1`
//! tool-length offset, so with zero offsets it equals the program coordinates.
//! `G53` moves and `G28`/`G30` home moves target absolute machine coordinates
//! regardless of `G90`/`G91`. Rapids, acceleration, `G93` timing, dwell, and
//! firmware compensation are not part of the display geometry. See the crate documentation of
//! [`Parser`] and the module docs for details.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod block;
mod compensation;
mod config;
mod error;
mod expression;
mod lexer;
mod parser;
mod planner;
mod program;
mod rewrite;
mod rotate;
mod scale;
mod state;

pub use compensation::{
    HeightCompensatedProgram, HeightCompensationError, HeightCompensationOptions,
    HeightCompensationStats, HeightCompensationWarning, HeightCompensationWarningKind,
    MAX_COMPENSATED_OUTPUT_LINES, MAX_HEIGHT_MAP_STEP_MM, MIN_HEIGHT_MAP_STEP_MM, OutsideMap,
    apply_height_compensation,
};
pub use config::{MAX_COORDINATE_MM, MIN_ARC_RADIUS_MM, ParserConfig};
pub use error::{ParseError, ParseErrorKind, ParseWarning, WarningKind};
pub use expression::{
    MAX_EXPRESSION_BYTES, MAX_EXPRESSION_DEPTH, MAX_NAMED_PARAMETERS, MAX_NUMERIC_PARAMETERS,
    MAX_PARAMETER_NAME_BYTES, ParameterError, Parameters,
};
pub use lexer::{
    DEFAULT_MAX_WORDS, LexedLine, Word, checksum_xor, lex_line, lex_line_with_parameters,
};
pub use parser::{Parser, parse_program, parse_program_with};
pub use program::{Program, ProgramLine, ProgramStats};
pub use rotate::{
    MAX_ROTATED_OUTPUT_LINES, MAX_ROTATION_ANGLE_DEG, RotatedProgram, RotationError,
    RotationOptions, RotationStats, RotationWarning, RotationWarningKind, apply_rotation,
};
pub use scale::{
    MAX_SCALE_FACTOR, MAX_SCALED_OUTPUT_LINES, MIN_SCALE_FACTOR, ScaleError, ScaleOptions,
    ScaleStats, ScaleWarning, ScaleWarningKind, ScaledProgram, apply_scale,
};
pub use state::{
    CannedCycleKind, CannedCycleParams, CoolantMode, DistanceMode, FeedMode, MotionMode,
    ParserState, PathControl, Plane, ProbeKind, RetractMode, Rotation, Scaling, SpindleMode,
    ToolLength,
};
