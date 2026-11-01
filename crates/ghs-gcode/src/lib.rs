//! G-code lexer, parser, parser-state emulation, and toolpath generation for
//! the grblHAL dialect.
//!
//! The crate is independent of the UI, transport, and controller session. It
//! turns program text into validated lines, a modal parser state, and display
//! geometry. Safety-critical command gating stays in `ghs-engine`.
