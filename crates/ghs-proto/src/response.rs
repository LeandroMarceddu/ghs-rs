//! Typed controller responses.

use ghs_core::{ProbeResult, SettingValue, ToolEntry};

use crate::line::ResponseParser;
use crate::offsets::{Offset, parse_offset_line, parse_probe_payload, parse_tool_table_line};
use crate::params::{
    CodeDescription, SettingDescription, SettingGroup, parse_code_description,
    parse_setting_description, parse_setting_group, parse_setting_line,
};
use crate::parser_state::ParserState;
use crate::status::{StatusReport, parse_status};
use crate::util::{bounded, leading_u16};

/// A response read back from a stored startup line, `$N<n>=<line>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupLine {
    /// Startup line index.
    pub index: u8,
    /// Stored line text.
    pub line: String,
}

/// Result of executing a stored startup line, `>...:ok` or `>...:error:<n>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupLineResult {
    /// Startup line description, usually `N0=...`.
    pub line: String,
    /// `Ok` or the controller error code.
    pub result: Result<(), u16>,
}

/// One decoded line from the controller.
///
/// Unknown lines are preserved in [`Response::Unknown`] rather than dropped, so
/// the console can show everything the controller sent.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "responses are short-lived events; boxing the status report would
              add an allocation on the hot telemetry path"
)]
pub enum Response {
    /// `ok`: one streamed line was accepted.
    Ok,
    /// `error:<n>`: one streamed line was rejected.
    Error(u16),
    /// `ALARM:<n>`: asynchronous alarm.
    Alarm(u16),
    /// `[MSG:...]`: informational message.
    Message(String),
    /// `<...>`: realtime status report.
    Status(StatusReport),
    /// Controller banner, for example `Grbl 1.1f ['$' for help]`.
    Banner(String),
    /// A bracketed line that has no dedicated type, for example `[VER:...]`.
    Feedback(String),
    /// `$<n>=<value>`: a setting value.
    Setting {
        /// Setting number.
        number: u16,
        /// Parsed value.
        value: SettingValue,
    },
    /// `[SETTING:...]` metadata from `$ES`.
    SettingDescription(SettingDescription),
    /// `[SETTINGGROUP:...]` metadata from `$EG`.
    SettingGroup(SettingGroup),
    /// `[ALARMCODE:...]` metadata from `$EA`.
    AlarmDescription(CodeDescription),
    /// `[ERRORCODE:...]` metadata from `$EE`.
    ErrorDescription(CodeDescription),
    /// One `$#` parameter.
    Offset(Offset),
    /// `[GC:...]` parser state.
    ParserState(ParserState),
    /// `[PRB:...]` probe result.
    Probe(ProbeResult),
    /// `[T:...]` tool-table entry from `$#`.
    ToolEntry(ToolEntry),
    /// `[HLP:...]` help text.
    Help(String),
    /// `[echo: ...]` echoed input.
    Echo(String),
    /// `$N<n>=...` stored startup line.
    StartupLine(StartupLine),
    /// `>...:ok` startup line result.
    StartupLineResult(StartupLineResult),
    /// Anything the parser does not recognise, preserved verbatim.
    Unknown(String),
}

impl Response {
    /// Returns `true` for the `ok` acknowledgement.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Response::Ok)
    }

    /// Returns `true` for responses that acknowledge exactly one streamed line.
    #[must_use]
    pub const fn is_ack(&self) -> bool {
        matches!(self, Response::Ok | Response::Error(_))
    }

    /// Decode one complete controller line.
    ///
    /// Never panics and never fails: anything unrecognised becomes
    /// [`Response::Unknown`]. Malformed status fields are recorded inside the
    /// [`StatusReport`] instead of discarding the report.
    #[must_use]
    pub fn parse(line: &str) -> Response {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Response::Unknown(String::new());
        }
        if trimmed.eq_ignore_ascii_case("ok") {
            return Response::Ok;
        }
        if has_prefix_ignore_ascii_case(trimmed, "error:") {
            return match leading_u16(&trimmed[6..]) {
                Some(code) => Response::Error(code),
                None => Response::Unknown(trimmed.to_string()),
            };
        }
        if has_prefix_ignore_ascii_case(trimmed, "alarm:") {
            return match leading_u16(&trimmed[6..]) {
                Some(code) => Response::Alarm(code),
                None => Response::Unknown(trimmed.to_string()),
            };
        }
        if trimmed.starts_with('<') {
            return match parse_status(trimmed) {
                Ok(report) => Response::Status(report),
                Err(_) => {
                    tracing::debug!(line = trimmed, "malformed status report");
                    Response::Unknown(trimmed.to_string())
                }
            };
        }
        if trimmed.starts_with('[') {
            return parse_bracketed(trimmed);
        }
        if trimmed.starts_with('>') {
            return match parse_startup_result(trimmed) {
                Some(result) => Response::StartupLineResult(result),
                None => Response::Unknown(trimmed.to_string()),
            };
        }
        if has_prefix_ignore_ascii_case(trimmed, "grbl") {
            return Response::Banner(trimmed.to_string());
        }
        if trimmed.starts_with('$') {
            if let Ok((number, value)) = parse_setting_line(trimmed) {
                return Response::Setting { number, value };
            }
            if let Some(startup) = parse_startup_line_read(trimmed) {
                return Response::StartupLine(startup);
            }
            return Response::Unknown(trimmed.to_string());
        }
        if let Ok(offset) = parse_offset_line(trimmed) {
            return Response::Offset(offset);
        }
        Response::Unknown(trimmed.to_string())
    }
}

fn has_prefix_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn parse_bracketed(line: &str) -> Response {
    let Some(inner) = line
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Response::Unknown(line.to_string());
    };
    let Some((tag, rest)) = inner.split_once(':') else {
        return Response::Feedback(line.to_string());
    };
    let tag = tag.trim();
    if tag.eq_ignore_ascii_case("MSG") {
        return Response::Message(rest.to_string());
    }
    if tag.eq_ignore_ascii_case("GC") {
        return match crate::parser_state::parse_parser_state(line) {
            Ok(state) => Response::ParserState(state),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("HLP") {
        return Response::Help(rest.to_string());
    }
    if tag.eq_ignore_ascii_case("ECHO") {
        return Response::Echo(rest.to_string());
    }
    if tag.eq_ignore_ascii_case("PRB") {
        return match parse_probe_payload(rest) {
            Some(probe) => Response::Probe(probe),
            None => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("SETTING") {
        return match parse_setting_description(line) {
            Ok(description) => Response::SettingDescription(description),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("SETTINGGROUP") {
        return match parse_setting_group(line) {
            Ok(group) => Response::SettingGroup(group),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("ALARMCODE") {
        return match parse_code_description(line) {
            Ok(description) => Response::AlarmDescription(description),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("ERRORCODE") {
        return match parse_code_description(line) {
            Ok(description) => Response::ErrorDescription(description),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if tag.eq_ignore_ascii_case("T") {
        return match parse_tool_table_line(line) {
            Ok(entry) => Response::ToolEntry(entry),
            Err(_) => Response::Unknown(line.to_string()),
        };
    }
    if let Ok(offset) = parse_offset_line(line) {
        return Response::Offset(offset);
    }
    Response::Feedback(line.to_string())
}

fn parse_startup_result(line: &str) -> Option<StartupLineResult> {
    let inner = line.strip_prefix('>')?;
    if let Some(prefix) = inner.strip_suffix(":ok") {
        return Some(StartupLineResult {
            line: bounded(prefix, 128),
            result: Ok(()),
        });
    }
    let index = inner.rfind(":error:")?;
    let (prefix, code) = inner.split_at(index);
    let code = code.trim_start_matches(":error:").parse::<u16>().ok()?;
    Some(StartupLineResult {
        line: bounded(prefix, 128),
        result: Err(code),
    })
}

fn parse_startup_line_read(line: &str) -> Option<StartupLine> {
    let rest = line.strip_prefix("$N")?;
    let (index, content) = rest.split_once('=')?;
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(StartupLine {
        index: index.parse::<u8>().ok()?,
        line: content.to_string(),
    })
}

/// Decode a whole chunk of controller bytes with a fresh [`ResponseParser`].
///
/// Convenience for tests and one-shot fixtures; the session should keep a
/// [`ResponseParser`] so partial lines survive chunk boundaries.
#[must_use]
pub fn parse_bytes(bytes: &[u8]) -> Vec<Response> {
    ResponseParser::new().push(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_core::{Position, Wcs};

    #[test]
    fn acknowledges_simple_lines() {
        assert_eq!(Response::parse("ok"), Response::Ok);
        assert_eq!(Response::parse("OK"), Response::Ok);
        assert_eq!(Response::parse("error:3"), Response::Error(3));
        assert_eq!(Response::parse("error:20"), Response::Error(20));
        assert_eq!(Response::parse("ALARM:1"), Response::Alarm(1));
        assert_eq!(Response::parse("alarm:2"), Response::Alarm(2));
        assert!(Response::parse("ok").is_ok());
        assert!(Response::parse("ok").is_ack());
        assert!(Response::parse("error:1").is_ack());
        assert!(!Response::parse("ALARM:1").is_ack());
    }

    #[test]
    fn malformed_error_lines_are_unknown() {
        assert!(matches!(Response::parse("error:"), Response::Unknown(_)));
        assert!(matches!(Response::parse("error:abc"), Response::Unknown(_)));
        assert!(matches!(Response::parse("ALARM:xyz"), Response::Unknown(_)));
    }

    #[test]
    fn parses_status_message_and_banner() {
        let status = Response::parse("<Idle|MPos:0.000,0.000,0.000>");
        assert!(matches!(status, Response::Status(_)));
        assert_eq!(
            Response::parse("[MSG:Info: homing]"),
            Response::Message("Info: homing".into())
        );
        assert_eq!(
            Response::parse("Grbl 1.1f ['$' for help]"),
            Response::Banner("Grbl 1.1f ['$' for help]".into())
        );
        assert!(matches!(
            Response::parse("grblHAL 1.1f ['$' for help]"),
            Response::Banner(_)
        ));
        assert!(matches!(Response::parse("<Idle"), Response::Unknown(_)));
    }

    #[test]
    fn parses_parser_state_and_help_and_echo() {
        let state = Response::parse("[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]");
        assert!(matches!(state, Response::ParserState(_)));
        assert_eq!(
            Response::parse("[HLP:$$ $# $G $I]"),
            Response::Help("$$ $# $G $I".into())
        );
        assert_eq!(
            Response::parse("[echo: G0 X0]"),
            Response::Echo(" G0 X0".into())
        );
    }

    #[test]
    fn parses_probe_results() {
        let probe = Response::parse("[PRB:-293.004,-16.995,-78.005:1]");
        match probe {
            Response::Probe(result) => {
                assert!(result.success);
                assert_eq!(result.position, Position::xyz(-293.004, -16.995, -78.005));
            }
            other => panic!("expected probe, got {other:?}"),
        }
        let failed = Response::parse("[PRB:0,0,0:0]");
        assert!(matches!(failed, Response::Probe(result) if !result.success));
        assert!(matches!(
            Response::parse("[PRB:garbage]"),
            Response::Unknown(_)
        ));
    }

    #[test]
    fn parses_offsets_bare_and_bracketed() {
        let g54 = Response::parse("[G54:-1031.000,-847.000,-2.000]");
        assert!(matches!(g54, Response::Offset(_)));
        let bare = Response::parse("G28:-1043.330,-801.000,-2.000");
        assert!(matches!(bare, Response::Offset(_)));
        let home = Response::parse("[HOME:-299.000,-353.000,-2.000:7]");
        match home {
            Response::Offset(offset) => {
                assert_eq!(offset.homed_mask, Some(ghs_core::Axes::xyz()));
            }
            other => panic!("expected offset, got {other:?}"),
        }
        assert_eq!(
            Response::parse("[G54:1,2,3]"),
            Response::Offset(Offset::new(
                crate::offsets::OffsetCode::Wcs(Wcs::G54),
                vec![1.0, 2.0, 3.0]
            ))
        );
    }

    #[test]
    fn parses_tool_table_entries() {
        let response = Response::parse("[T:1|0.000,0.000,-10.000|2.000|6,0,0|Face mill|2]");
        match response {
            Response::ToolEntry(entry) => {
                assert_eq!(entry.number, 1);
                assert_eq!(entry.name.as_deref(), Some("Face mill"));
                assert_eq!(entry.pocket, Some(2));
            }
            other => panic!("expected tool entry, got {other:?}"),
        }
        assert!(matches!(
            Response::parse("[T:bad|0,0,0|1.0]"),
            Response::Unknown(_)
        ));
    }

    #[test]
    fn parses_settings_and_startup_lines() {
        assert_eq!(
            Response::parse("$100=250.000"),
            Response::Setting {
                number: 100,
                value: SettingValue::Float(250.0)
            }
        );
        assert_eq!(
            Response::parse("$22=1"),
            Response::Setting {
                number: 22,
                value: SettingValue::Int(1)
            }
        );
        assert_eq!(
            Response::parse("$N0=G54"),
            Response::StartupLine(StartupLine {
                index: 0,
                line: "G54".into()
            })
        );
        assert_eq!(
            Response::parse(">N0=G54:ok"),
            Response::StartupLineResult(StartupLineResult {
                line: "N0=G54".into(),
                result: Ok(())
            })
        );
        assert_eq!(
            Response::parse(">N1=G0 X1:error:20"),
            Response::StartupLineResult(StartupLineResult {
                line: "N1=G0 X1".into(),
                result: Err(20)
            })
        );
    }

    #[test]
    fn parses_runtime_enumeration() {
        let setting = Response::parse(
            "[SETTING:110|1|X-axis maximum rate|mm/min|6|#####0.000|1|1000000|0|N/A]",
        );
        assert!(matches!(setting, Response::SettingDescription(_)));

        let group = Response::parse("[SETTINGGROUP:2|1|Spindle]");
        assert!(matches!(group, Response::SettingGroup(_)));

        let alarm = Response::parse("[ALARMCODE:1|Hard limit|Hard limit triggered.]");
        assert!(matches!(alarm, Response::AlarmDescription(_)));

        let error = Response::parse("[ERRORCODE:20|Unsupported command|Invalid g-code.]");
        assert!(matches!(error, Response::ErrorDescription(_)));

        assert!(matches!(
            Response::parse("[SETTING:not enough]"),
            Response::Unknown(_)
        ));
    }

    #[test]
    fn unrecognised_lines_are_preserved() {
        assert_eq!(
            Response::parse("[VER:1.1f.20240604:]"),
            Response::Feedback("[VER:1.1f.20240604:]".into())
        );
        assert!(matches!(
            Response::parse("[T:1|0,0,0|1.0]"),
            Response::ToolEntry(_)
        ));
        assert_eq!(
            Response::parse("[PIN:map]"),
            Response::Feedback("[PIN:map]".into())
        );
        assert_eq!(
            Response::parse("something else"),
            Response::Unknown("something else".into())
        );
        assert_eq!(Response::parse("   "), Response::Unknown(String::new()));
        assert_eq!(
            Response::parse("$N=G54"),
            Response::Unknown("$N=G54".into())
        );
    }

    #[test]
    fn parse_bytes_handles_multiple_lines() {
        let responses = parse_bytes(b"ok\r\nerror:1\r\n<Idle|MPos:0,0,0>\r\n");
        assert_eq!(responses.len(), 3);
        assert_eq!(responses[0], Response::Ok);
        assert_eq!(responses[1], Response::Error(1));
        assert!(matches!(responses[2], Response::Status(_)));
    }
}
