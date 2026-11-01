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
//! source line -> lexer::lex_line -> Block -> Planner (validate + plan)
//!             -> Parser::execute (modal state + Toolpath emission)
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
//! Unsupported codes are rejected with a structured error instead of being
//! silently ignored, so a validation view can explain why a program will not
//! run.
//!
//! # Deferred (rejected, not emulated)
//!
//! Expressions (`#` parameters, `[...]`), `IF`/`WHILE` flow control, `O`-word
//! subprograms, `M98`/`M99`, cutter radius compensation (`G41`/`G42`), lathe
//! cycles, `G84`/`G87`/`G88` tapping, `G90.1`/`G91.1` absolute arc centers,
//! and extra axes (`A`, `B`, `C`, `U`, `V`, `W`). These return
//! [`ParseErrorKind::UnsupportedExpression`] or
//! [`ParseErrorKind::UnsupportedCode`]. VIEW-02 adds an expression evaluator as
//! a pre-pass over [`LexedLine`] words; [`Parser::parse_lexed`] is the hook.
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
mod config;
mod error;
mod lexer;
mod parser;
mod planner;
mod program;
mod state;

pub use config::{MAX_COORDINATE_MM, MIN_ARC_RADIUS_MM, ParserConfig};
pub use error::{ParseError, ParseErrorKind, ParseWarning, WarningKind};
pub use lexer::{DEFAULT_MAX_WORDS, LexedLine, Word, checksum_xor, lex_line};
pub use parser::{Parser, parse_program, parse_program_with};
pub use program::{Program, ProgramLine, ProgramStats};
pub use state::{
    CannedCycleKind, CannedCycleParams, CoolantMode, DistanceMode, FeedMode, MotionMode,
    ParserState, PathControl, Plane, ProbeKind, RetractMode, Rotation, Scaling, SpindleMode,
    ToolLength,
};
