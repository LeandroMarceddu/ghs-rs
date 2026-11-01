//! `$#` parameter parsing: work offsets, `G28`/`G30`, `G92`, tool offsets and
//! probe results.

use std::fmt;

use ghs_core::{Axes, Position, ProbeResult, Wcs};

use crate::error::ParseError;
use crate::util::{bounded, parse_position};

/// Code of one `$#` parameter line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffsetCode {
    /// Work coordinate system offset (`G54` .. `G59.3`).
    Wcs(Wcs),
    /// `G28` predefined position.
    G28,
    /// `G30` predefined position.
    G30,
    /// `G92` coordinate offset.
    G92,
    /// Tool length offset (`TLO`).
    Tlo,
    /// Home position (`HOME`, grblHAL extension).
    Home,
    /// `G51` scaling factors.
    Scaling,
    /// Tool length reference (`TLR`, grblHAL extension).
    Tlr,
    /// Any other code, kept verbatim.
    Other(String),
}

impl OffsetCode {
    /// Canonical code text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            OffsetCode::Wcs(wcs) => wcs.as_str(),
            OffsetCode::G28 => "G28",
            OffsetCode::G30 => "G30",
            OffsetCode::G92 => "G92",
            OffsetCode::Tlo => "TLO",
            OffsetCode::Home => "HOME",
            OffsetCode::Scaling => "G51",
            OffsetCode::Tlr => "TLR",
            OffsetCode::Other(code) => code,
        }
    }
}

impl fmt::Display for OffsetCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One decoded `$#` parameter line.
#[derive(Debug, Clone, PartialEq)]
pub struct Offset {
    /// Parameter code.
    pub code: OffsetCode,
    /// Values in the order reported (usually one per axis, millimetres).
    pub values: Vec<f64>,
    /// Homed-axis mask reported with `HOME`.
    pub homed_mask: Option<Axes>,
}

impl Offset {
    /// Build an offset from a code and values.
    #[must_use]
    pub fn new(code: OffsetCode, values: Vec<f64>) -> Self {
        Offset {
            code,
            values,
            homed_mask: None,
        }
    }

    /// Interpret the values as a position over the first axes in wire order.
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        let axes = Axes::from_iter(ghs_core::Axis::ALL.into_iter().take(self.values.len()));
        Position::from_values(axes, &self.values)
    }

    /// First value, for single-value parameters such as `TLR`.
    #[must_use]
    pub fn single_value(&self) -> Option<f64> {
        self.values.first().copied()
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.code)?;
        for (index, value) in self.values.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{value:.3}")?;
        }
        if let Some(mask) = self.homed_mask {
            write!(f, ":{}", mask.bits())?;
        }
        Ok(())
    }
}

/// Decoded `$#` response.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Offsets {
    /// Work coordinate offsets, indexed by [`Wcs::index`].
    pub wcs: [Option<Position>; 9],
    /// `G28` predefined position.
    pub g28: Option<Position>,
    /// `G30` predefined position.
    pub g30: Option<Position>,
    /// `G92` coordinate offset.
    pub g92: Option<Position>,
    /// Home position in machine coordinates.
    pub home: Option<Position>,
    /// Axes with a valid home position.
    pub home_mask: Option<Axes>,
    /// Tool length offset; one value or one value per axis.
    pub tlo: Option<Vec<f64>>,
    /// Tool length reference offset.
    pub tlr: Option<f64>,
    /// `G51` scaling factors.
    pub scaling: Option<Position>,
    /// Last probe result.
    pub probe: Option<ProbeResult>,
    /// Parameters with an unrecognised code.
    pub other: Vec<Offset>,
}

impl Offsets {
    /// Offset for one work coordinate system.
    #[must_use]
    pub fn wcs(&self, wcs: Wcs) -> Option<Position> {
        self.wcs[wcs.index()]
    }

    /// Apply one decoded parameter.
    pub fn apply(&mut self, offset: Offset) {
        match &offset.code {
            OffsetCode::Wcs(wcs) => {
                if let Some(position) = offset.position() {
                    self.wcs[wcs.index()] = Some(position);
                }
            }
            OffsetCode::G28 => self.g28 = offset.position(),
            OffsetCode::G30 => self.g30 = offset.position(),
            OffsetCode::G92 => self.g92 = offset.position(),
            OffsetCode::Home => {
                self.home = offset.position();
                self.home_mask = offset.homed_mask;
            }
            OffsetCode::Tlo => self.tlo = Some(offset.values.clone()),
            OffsetCode::Tlr => self.tlr = offset.single_value(),
            OffsetCode::Scaling => self.scaling = offset.position(),
            OffsetCode::Other(_) => self.other.push(offset),
        }
    }
}

/// Parse one `$#` parameter line, bracketed or bare.
///
/// # Errors
///
/// Returns [`ParseError`] when the code is not a known parameter or a value is
/// not a finite number.
pub fn parse_offset_line(line: &str) -> Result<Offset, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let (code, payload) = inner
        .split_once(':')
        .ok_or(ParseError::Expected("a parameter code and values"))?;
    parse_offset_payload(code.trim(), payload)
}

/// Parse a parameter code and payload.
///
/// # Errors
///
/// Returns [`ParseError`] for an unknown code or a malformed value list.
pub fn parse_offset_payload(code: &str, payload: &str) -> Result<Offset, ParseError> {
    let code = parse_code(code)?;
    if code == OffsetCode::Home {
        let (values, mask) = match payload.rsplit_once(':') {
            Some((values, mask)) => (values, Some(mask)),
            None => (payload, None),
        };
        let mut offset = Offset::new(code, parse_values(values)?);
        offset.homed_mask = mask
            .and_then(|mask| mask.trim().parse::<u16>().ok())
            .map(Axes::from_bits_retain);
        return Ok(offset);
    }
    Ok(Offset::new(code, parse_values(payload)?))
}

fn parse_code(code: &str) -> Result<OffsetCode, ParseError> {
    let upper = code.trim().to_ascii_uppercase();
    if let Some(wcs) = Wcs::parse(&upper) {
        return Ok(OffsetCode::Wcs(wcs));
    }
    match upper.as_str() {
        "G28" => Ok(OffsetCode::G28),
        "G30" => Ok(OffsetCode::G30),
        "G92" | "92" => Ok(OffsetCode::G92),
        "TLO" => Ok(OffsetCode::Tlo),
        "HOME" => Ok(OffsetCode::Home),
        "G51" => Ok(OffsetCode::Scaling),
        "TLR" => Ok(OffsetCode::Tlr),
        _ => Err(ParseError::Expected("a known offset code")),
    }
}

fn parse_values(payload: &str) -> Result<Vec<f64>, ParseError> {
    if payload.trim().is_empty() {
        return Err(ParseError::Expected("at least one value"));
    }
    let mut values = Vec::new();
    for part in payload.split(',') {
        if values.len() == 9 {
            return Err(ParseError::InvalidValue("too many values".into()));
        }
        let value = part
            .trim()
            .parse::<f64>()
            .map_err(|_| ParseError::InvalidNumber(bounded(part, 32)))?;
        if !value.is_finite() {
            return Err(ParseError::InvalidValue("value is not finite".into()));
        }
        values.push(value);
    }
    Ok(values)
}

/// Parse the payload of a `[PRB:...]` line or the status `PRB:` field.
///
/// The coordinate list is mandatory. A trailing `:0`/`:1` success flag and an
/// optional work coordinate system are recognised when present. The status
/// field carries no success flag, so a result without one is reported as
/// unsuccessful; only an explicit `:1` marks a probe as successful.
#[must_use]
pub fn parse_probe_payload(payload: &str) -> Option<ProbeResult> {
    let mut parts: Vec<&str> = payload.split(':').collect();
    let mut success = false;
    if let Some(last) = parts.last().copied() {
        match last.trim() {
            "0" => {
                parts.pop();
            }
            "1" => {
                success = true;
                parts.pop();
            }
            _ => {}
        }
    }
    let mut wcs = None;
    if let Some(last) = parts.last().copied()
        && let Some(candidate) = Wcs::parse(last)
    {
        wcs = Some(candidate);
        parts.pop();
    }
    let coordinates = parts.join(":");
    let position = parse_position(&coordinates).ok()?;
    Some(ProbeResult {
        success,
        position,
        wcs,
    })
}

/// Parse a `[T:...]` tool-table line from `$#`.
///
/// The grblHAL form is `[T:<tool>|<x,y,z>|<radius>|<flutes>|<name>|<pocket>]`;
/// the trailing fields are optional and older builds omit them.
///
/// # Errors
///
/// Returns [`ParseError`] when the line is not a tool-table entry or the
/// offset/radius is malformed.
pub fn parse_tool_table_line(line: &str) -> Result<ghs_core::ToolEntry, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let (tag, rest) = inner
        .split_once(':')
        .ok_or(ParseError::Expected("a 'T:' tool table line"))?;
    if !tag.trim().eq_ignore_ascii_case("T") {
        return Err(ParseError::Expected("a 'T:' tool table line"));
    }
    let fields: Vec<&str> = rest.split('|').collect();
    if fields.len() < 3 {
        return Err(ParseError::Expected("tool number, offset and radius"));
    }
    let number = fields[0]
        .trim()
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidNumber(bounded(fields[0], 16)))?;
    let offset =
        parse_position(fields[1]).map_err(|reason| ParseError::InvalidValue(reason.to_string()))?;
    let radius_mm = crate::util::parse_f64(fields[2]);
    let name = fields
        .get(4)
        .map(|field| field.trim())
        .filter(|field| !field.is_empty())
        .map(|field| bounded(field, 128));
    let pocket = fields
        .get(5)
        .and_then(|field| field.trim().parse::<u32>().ok());
    Ok(ghs_core::ToolEntry {
        number,
        radius_mm,
        offset,
        name,
        pocket,
    })
}

/// Parse a complete `$#` response into typed parameters.
///
/// Unrecognised lines are ignored; a malformed value never aborts the whole
/// response.
#[must_use]
pub fn parse_offsets<S: AsRef<str>>(lines: &[S]) -> Offsets {
    let mut offsets = Offsets::default();
    for line in lines {
        let trimmed = line.as_ref().trim();
        let inner = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(trimmed);
        let Some((code, payload)) = inner.split_once(':') else {
            continue;
        };
        if code.trim().eq_ignore_ascii_case("PRB") {
            if let Some(probe) = parse_probe_payload(payload) {
                offsets.probe = Some(probe);
            }
            continue;
        }
        if let Ok(offset) = parse_offset_payload(code.trim(), payload) {
            offsets.apply(offset);
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFSETS: &[&str] = &[
        "[G54:-357.000,-757.000,-148.306]",
        "[G55:0.000,0.000,0.000]",
        "[G56:0.000,0.000,0.000]",
        "[G57:0.000,0.000,0.000]",
        "[G58:0.000,0.000,0.000]",
        "[G59:0.000,0.000,0.000]",
        "[G59.1:0.000,0.000,0.000]",
        "[G59.2:0.000,0.000,0.000]",
        "[G59.3:0.000,0.000,0.000]",
        "[G28:0.000,0.000,0.000]",
        "[G30:0.000,0.000,0.000]",
        "[G92:0.000,0.000,0.000]",
        "[HOME:-299.000,-353.000,-2.000:7]",
        "[TLO:0.000,0.000,3.000]",
        "[TLR:12.500]",
        "[G51:1.000,2.000,3.000]",
        "[PRB:-293.004,-16.995,-78.005:1]",
    ];

    #[test]
    fn parses_complete_offset_response() {
        let offsets = parse_offsets(OFFSETS);
        assert_eq!(
            offsets.wcs(Wcs::G54),
            Some(Position::xyz(-357.0, -757.0, -148.306))
        );
        assert_eq!(offsets.wcs(Wcs::G59_3), Some(Position::zero()));
        assert_eq!(offsets.g28, Some(Position::zero()));
        assert_eq!(offsets.g30, Some(Position::zero()));
        assert_eq!(offsets.g92, Some(Position::zero()));
        assert_eq!(offsets.home, Some(Position::xyz(-299.0, -353.0, -2.0)));
        assert_eq!(offsets.home_mask, Some(Axes::from_bits_retain(7)));
        assert_eq!(offsets.tlo.as_deref(), Some(&[0.0, 0.0, 3.0][..]));
        assert_eq!(offsets.tlr, Some(12.5));
        assert_eq!(offsets.scaling, Some(Position::xyz(1.0, 2.0, 3.0)));
        assert_eq!(
            offsets.probe,
            Some(ProbeResult::new(
                true,
                Position::xyz(-293.004, -16.995, -78.005)
            ))
        );
    }

    #[test]
    fn parses_bare_and_bracketed_lines() {
        let bare = parse_offset_line("G54:-1031.000,-847.000,-2.000").expect("valid");
        assert_eq!(bare.code, OffsetCode::Wcs(Wcs::G54));
        assert_eq!(bare.values, vec![-1031.0, -847.0, -2.0]);
        assert_eq!(
            parse_offset_line("[G28:-1043.330,-801.000,-2.000]")
                .expect("valid")
                .code,
            OffsetCode::G28
        );
        assert_eq!(bare.to_string(), "G54:-1031.000,-847.000,-2.000");
    }

    #[test]
    fn tlo_accepts_one_or_many_values() {
        let single = parse_offset_line("[TLO:0.000]").expect("valid");
        assert_eq!(single.single_value(), Some(0.0));
        assert_eq!(
            single.position().map(|position| position.axes()),
            Some(ghs_core::Axes::from(ghs_core::Axis::X))
        );
        let multi = parse_offset_line("[TLO:0.000,0.000,-14.442]").expect("valid");
        assert_eq!(multi.values.len(), 3);
        assert_eq!(multi.position(), Some(Position::xyz(0.0, 0.0, -14.442)));
    }

    #[test]
    fn malformed_offsets_are_rejected() {
        assert!(parse_offset_line("[G54:1,two,3]").is_err());
        assert!(parse_offset_line("[G54:]").is_err());
        assert!(parse_offset_line("[XYZ:1,2,3]").is_err());
        assert!(parse_offset_line("no colon").is_err());
        assert!(parse_offset_line("[G54:NaN,0,0]").is_err());
        let home_without_mask = parse_offset_line("[HOME:0,0,0:]").expect("tolerated");
        assert_eq!(home_without_mask.homed_mask, None);
    }

    #[test]
    fn probe_payload_forms() {
        let with_flag = parse_probe_payload("1.000,2.000,-3.000:1").expect("valid");
        assert!(with_flag.success);
        let without_flag = parse_probe_payload("1.000,2.000,-3.000").expect("valid");
        assert!(!without_flag.success);
        let with_wcs = parse_probe_payload("1,2,3:G55").expect("valid");
        assert_eq!(with_wcs.wcs, Some(Wcs::G55));
        assert!(parse_probe_payload("").is_none());
        assert!(parse_probe_payload("bad").is_none());
    }

    #[test]
    fn parses_tool_table_entries() {
        let entry = parse_tool_table_line("[T:3|10.000,20.000,-5.000|3.175]").expect("valid");
        assert_eq!(entry.number, 3);
        assert_eq!(entry.radius_mm, Some(3.175));
        assert_eq!(entry.offset, Position::xyz(10.0, 20.0, -5.0));

        let extended = parse_tool_table_line("[T:7|0.000,0.000,-12.000|1.500|6,0,0|End mill|4]")
            .expect("valid");
        assert_eq!(extended.name.as_deref(), Some("End mill"));
        assert_eq!(extended.pocket, Some(4));

        assert!(parse_tool_table_line("[T:bad|0,0,0|1.0]").is_err());
        assert!(parse_tool_table_line("[T:1|0,0,0]").is_err());
        assert!(parse_tool_table_line("[G54:0,0,0]").is_err());
    }

    #[test]
    fn accepts_the_bare_92_offset_form() {
        let offset = parse_offset_line("92:1.000,2.000,3.000").expect("valid");
        assert_eq!(offset.code, OffsetCode::G92);
        assert_eq!(offset.position(), Some(Position::xyz(1.0, 2.0, 3.0)));
        let offsets = parse_offsets(&["92:1,2,3"]);
        assert_eq!(offsets.g92, Some(Position::xyz(1.0, 2.0, 3.0)));
    }

    #[test]
    fn unknown_lines_are_ignored_by_bulk_parser() {
        let offsets = parse_offsets(&["[T:1|0,0,0|1.0]", "garbage", "[G55:1,2,3]"]);
        assert_eq!(offsets.wcs(Wcs::G55), Some(Position::xyz(1.0, 2.0, 3.0)));
        assert_eq!(offsets.wcs(Wcs::G54), None);
    }
}
