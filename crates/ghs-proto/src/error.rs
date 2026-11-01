//! Typed errors for protocol parsing and command encoding.

/// Failure parsing a controller response.
///
/// Parsing a complete response never panics. Field-level problems are recorded
/// in a [`crate::StatusReport`]'s `malformed` list instead of failing the whole
/// report; this error covers the cases where a line cannot be interpreted at
/// all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// A required delimiter or prefix was missing.
    #[error("expected {0}")]
    Expected(&'static str),
    /// A numeric field did not contain a valid number.
    #[error("invalid number: {0}")]
    InvalidNumber(String),
    /// A field had the wrong shape or an out-of-range value.
    #[error("invalid value: {0}")]
    InvalidValue(String),
    /// The line exceeded the parser's maximum length.
    #[error("line exceeds the maximum length of {0} bytes")]
    LineTooLong(usize),
}

/// Failure encoding a command.
///
/// Encoders reject anything that could inject bytes into the controller stream
/// (control characters, non-ASCII bytes) and anything that cannot be expressed
/// safely (for example a jog with no axes).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EncodeError {
    /// A control or non-ASCII character was found in a command line.
    #[error("invalid character 0x{byte:02X} at byte offset {index}")]
    InvalidCharacter {
        /// Byte offset of the offending character.
        index: usize,
        /// Offending byte value.
        byte: u8,
    },
    /// A command line exceeded the configured maximum length.
    #[error("line is {len} bytes; the limit is {limit}")]
    LineTooLong {
        /// Line length in bytes.
        len: usize,
        /// Maximum accepted length.
        limit: usize,
    },
    /// A value was syntactically unusable.
    #[error("invalid value: {0}")]
    InvalidValue(String),
    /// A setting number was outside the accepted range.
    #[error("setting number {0} is out of range")]
    InvalidSettingNumber(u16),
    /// A startup-line index was outside the accepted range.
    #[error("startup line index {0} is out of range")]
    InvalidStartupLine(u8),
    /// A jog command had no axis words.
    #[error("a jog requires at least one axis")]
    EmptyJog,
    /// The jog axis mask and the distance position disagree.
    #[error("jog axis mask does not match the axes in the distance")]
    JogAxisMismatch,
    /// A jog coordinate was not finite.
    #[error("jog coordinates must be finite")]
    InvalidAxisValue,
    /// The jog feed rate was not a positive, finite, plausible value.
    #[error("jog feed must be greater than 0 and at most {0} mm/min")]
    InvalidFeed(f64),
}
