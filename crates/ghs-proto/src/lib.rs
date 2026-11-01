//! Sans-IO grblHAL protocol implementation.
//!
//! The crate consumes bytes and emits typed events, and encodes typed commands
//! into bytes. It performs no I/O, owns no threads, and reads no clocks.
//!
//! The main entry points are:
//!
//! - [`ResponseParser`] / [`LineParser`] for controller output.
//! - [`Response`] and [`StatusReport`] for typed decoded lines.
//! - [`RealtimeCommand`], [`SystemCommand`] and [`JogCommand`] for encoding.
//! - [`Streamer`] for character-counting, buffer-safe job streaming.
//!
//! ```
//! use ghs_proto::{Response, ResponseParser};
//!
//! let mut parser = ResponseParser::new();
//! let responses = parser.push(b"<Idle|MPos:0.000,0.000,0.000|FS:0,0>\r\nok\r\n");
//! assert_eq!(responses.len(), 2);
//! assert!(matches!(responses[0], Response::Status(_)));
//! assert_eq!(responses[1], Response::Ok);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod command;
mod error;
mod error_state;
mod line;
mod offsets;
mod params;
mod parser_state;
mod response;
mod status;
mod streamer;
mod util;

pub use command::{
    ExtendedCommand, JogCommand, JogMode, MAX_COMMAND_LINE, MAX_JOG_FEED_MM_MIN,
    MAX_SETTING_NUMBER, MAX_SETTING_VALUE, MAX_STARTUP_LINE, MAX_STARTUP_LINE_INDEX,
    RealtimeCommand, ResetTarget, SystemCommand, sanitize_line, validate_line,
};
pub use error::{EncodeError, ParseError};
pub use error_state::ErrorState;
pub use line::{DEFAULT_MAX_LINE_LEN, Line, LineParser, ResponseParser};
pub use offsets::{
    Offset, OffsetCode, Offsets, parse_offset_line, parse_offset_payload, parse_offsets,
    parse_probe_payload, parse_tool_table_line,
};
pub use params::{
    CodeDescription, CodeTable, SettingDescription, SettingGroup, SettingType,
    parse_code_description, parse_code_descriptions, parse_setting_description,
    parse_setting_group, parse_setting_line, parse_settings,
};
pub use parser_state::{
    CoolantMode, DistanceMode, FeedRateMode, ParserState, Plane, SpindleMode, parse_gc_word_list,
    parse_parser_state,
};
pub use response::{Response, StartupLine, StartupLineResult, parse_bytes};
pub use status::{
    AccessoryState, HomingStatus, MalformedField, SdStatus, StatusReport, parse_status,
};
pub use streamer::{
    AckedLine, DEFAULT_RX_BUFFER_SIZE, MAX_QUEUED_LINES, Streamer, StreamerConfig, StreamerError,
};

pub use ghs_core::{Capabilities, ControllerInfo, ToolEntry};

/// Parse a `$I` or `$I+` response into controller capabilities.
///
/// Lines may be bracketed or bare; unknown lines are ignored, so an incomplete
/// or noisy response still yields a usable [`ControllerInfo`].
#[must_use]
pub fn parse_controller_info<S: AsRef<str>>(lines: &[S]) -> ControllerInfo {
    ghs_core::parse_controller_info(lines)
}

/// Version of the protocol crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
