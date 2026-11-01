//! `$G` parser-state report parsing.

use std::fmt;

use ghs_core::{Units, Wcs};

use crate::error::ParseError;
use crate::util::{bounded, parse_f64, parse_u32};

/// Maximum number of words kept from a `[GC:...]` report.
const MAX_WORDS: usize = 64;

/// Selected plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    /// `G17`, XY plane.
    XY,
    /// `G18`, ZX plane.
    ZX,
    /// `G19`, YZ plane.
    YZ,
}

impl Plane {
    /// The selecting G-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Plane::XY => "G17",
            Plane::ZX => "G18",
            Plane::YZ => "G19",
        }
    }
}

impl fmt::Display for Plane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Distance mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMode {
    /// `G90`, absolute coordinates.
    Absolute,
    /// `G91`, incremental coordinates.
    Incremental,
}

impl DistanceMode {
    /// The selecting G-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            DistanceMode::Absolute => "G90",
            DistanceMode::Incremental => "G91",
        }
    }
}

/// Feed rate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedRateMode {
    /// `G93`, inverse time.
    InverseTime,
    /// `G94`, units per minute.
    UnitsPerMinute,
    /// `G95`, units per revolution.
    UnitsPerRevolution,
}

impl FeedRateMode {
    /// The selecting G-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            FeedRateMode::InverseTime => "G93",
            FeedRateMode::UnitsPerMinute => "G94",
            FeedRateMode::UnitsPerRevolution => "G95",
        }
    }
}

/// Spindle modal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpindleMode {
    /// `M5`, spindle stopped.
    Off,
    /// `M3`, spindle clockwise.
    Clockwise,
    /// `M4`, spindle counter-clockwise.
    CounterClockwise,
}

impl SpindleMode {
    /// The selecting M-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SpindleMode::Off => "M5",
            SpindleMode::Clockwise => "M3",
            SpindleMode::CounterClockwise => "M4",
        }
    }
}

/// Coolant modal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoolantMode {
    /// `M9`, all coolant off.
    Off,
    /// `M7`, mist coolant.
    Mist,
    /// `M8`, flood coolant.
    Flood,
}

impl CoolantMode {
    /// The selecting M-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            CoolantMode::Off => "M9",
            CoolantMode::Mist => "M7",
            CoolantMode::Flood => "M8",
        }
    }
}

/// Decoded grblHAL `$G` parser state, `[GC:...]`.
///
/// Raw words are preserved in [`ParserState::words`] for anything the typed
/// fields do not cover, so newer controllers never break the parser. The typed
/// fields are intentionally limited to the modal state the engine needs for
/// safety checks; the full G-code model belongs to `ghs-gcode`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParserState {
    /// Every reported word, upper-cased and without a trailing period.
    pub words: Vec<String>,
    /// Active motion mode word, for example `"G1"` or `"G38.2"`.
    pub motion: Option<String>,
    /// Active work coordinate system.
    pub wcs: Option<Wcs>,
    /// Active plane.
    pub plane: Option<Plane>,
    /// Active units mode.
    pub units: Option<Units>,
    /// Active distance mode.
    pub distance_mode: Option<DistanceMode>,
    /// Active feed rate mode.
    pub feed_rate_mode: Option<FeedRateMode>,
    /// Active spindle state.
    pub spindle: Option<SpindleMode>,
    /// Active coolant state.
    pub coolant: Option<CoolantMode>,
    /// Selected tool.
    pub tool: Option<u32>,
    /// Programmed feed rate, in the active units per minute.
    pub feed: Option<f64>,
    /// Programmed spindle speed.
    pub spindle_rpm: Option<f64>,
    /// Tool length offset mode word (`G43.1` or `G49`).
    pub tool_length_offset: Option<String>,
    /// Scaling state from `G50`/`G51`.
    pub scaling: Option<bool>,
}

/// Parse a `$G` response, either `[GC:...]` or `GC:...`.
///
/// # Errors
///
/// Returns [`ParseError::Expected`] when the line is not a parser-state report.
pub fn parse_parser_state(line: &str) -> Result<ParserState, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let prefix = inner
        .get(..3)
        .ok_or(ParseError::Expected("a 'GC:' parser-state report"))?;
    if !prefix.eq_ignore_ascii_case("GC:") {
        return Err(ParseError::Expected("a 'GC:' parser-state report"));
    }
    Ok(parse_gc_word_list(&inner[3..]))
}

/// Parse the whitespace-separated words after `GC:`.
#[must_use]
pub fn parse_gc_word_list(words: &str) -> ParserState {
    let mut state = ParserState::default();
    for raw in words.split_whitespace().take(MAX_WORDS) {
        let word = raw.trim_end_matches('.').to_ascii_uppercase();
        if word.len() < 2 {
            continue;
        }
        state.words.push(bounded(&word, 32));
        let mut chars = word.chars();
        let Some(letter) = chars.next() else {
            continue;
        };
        let value: String = chars.collect();
        let value = value.trim();
        match letter {
            'G' => state.apply_g(value),
            'M' => state.apply_m(value),
            'T' => state.tool = parse_u32(value),
            'F' => state.feed = parse_f64(value),
            'S' => state.spindle_rpm = parse_f64(value),
            _ => {}
        }
    }
    state
}

impl ParserState {
    fn apply_g(&mut self, value: &str) {
        match value {
            "0" | "1" | "2" | "3" | "5" | "5.1" | "33" | "38.2" | "38.3" | "38.4" | "38.5"
            | "73" | "76" | "80" | "81" | "82" | "83" | "84" | "85" | "86" | "89" => {
                self.motion = Some(format!("G{value}"));
            }
            "17" => self.plane = Some(Plane::XY),
            "18" => self.plane = Some(Plane::ZX),
            "19" => self.plane = Some(Plane::YZ),
            "20" => self.units = Some(Units::Inch),
            "21" => self.units = Some(Units::Mm),
            "43" | "43.1" | "49" => self.tool_length_offset = Some(format!("G{value}")),
            "50" => self.scaling = Some(false),
            "51" => self.scaling = Some(true),
            "90" => self.distance_mode = Some(DistanceMode::Absolute),
            "91" => self.distance_mode = Some(DistanceMode::Incremental),
            "93" => self.feed_rate_mode = Some(FeedRateMode::InverseTime),
            "94" => self.feed_rate_mode = Some(FeedRateMode::UnitsPerMinute),
            "95" => self.feed_rate_mode = Some(FeedRateMode::UnitsPerRevolution),
            "54" | "55" | "56" | "57" | "58" | "59" | "59.1" | "59.2" | "59.3" => {
                self.wcs = Wcs::parse(&format!("G{value}"));
            }
            _ => {}
        }
    }

    fn apply_m(&mut self, value: &str) {
        match value {
            "3" => self.spindle = Some(SpindleMode::Clockwise),
            "4" => self.spindle = Some(SpindleMode::CounterClockwise),
            "5" => self.spindle = Some(SpindleMode::Off),
            "7" => self.coolant = Some(CoolantMode::Mist),
            "8" => self.coolant = Some(CoolantMode::Flood),
            "9" => self.coolant = Some(CoolantMode::Off),
            _ => {}
        }
    }
}

impl fmt::Display for ParserState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, word) in self.words.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            f.write_str(word)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reference_parser_state() {
        let state = parse_parser_state("[GC:G0 G54 G17 G21 G90 G94 G43.1 G98 G50 M5 M9 T0 F0 S0.]")
            .expect("valid");
        assert_eq!(state.motion.as_deref(), Some("G0"));
        assert_eq!(state.wcs, Some(Wcs::G54));
        assert_eq!(state.plane, Some(Plane::XY));
        assert_eq!(state.units, Some(Units::Mm));
        assert_eq!(state.distance_mode, Some(DistanceMode::Absolute));
        assert_eq!(state.feed_rate_mode, Some(FeedRateMode::UnitsPerMinute));
        assert_eq!(state.spindle, Some(SpindleMode::Off));
        assert_eq!(state.coolant, Some(CoolantMode::Off));
        assert_eq!(state.tool, Some(0));
        assert_eq!(state.feed, Some(0.0));
        assert_eq!(state.spindle_rpm, Some(0.0));
        assert_eq!(state.tool_length_offset.as_deref(), Some("G43.1"));
        assert_eq!(state.scaling, Some(false));
        assert!(state.words.contains(&"G43.1".to_string()));
        assert_eq!(state.words.len(), 14);
        assert_eq!(
            state.to_string(),
            "G0 G54 G17 G21 G90 G94 G43.1 G98 G50 M5 M9 T0 F0 S0"
        );
    }

    #[test]
    fn parses_inch_incremental_mist_state() {
        let state = parse_parser_state("[GC:G2 G55 G18 G20 G91 G93 G49 G51 M4 M7 T5 F1.5 S12000]")
            .expect("valid");
        assert_eq!(state.motion.as_deref(), Some("G2"));
        assert_eq!(state.wcs, Some(Wcs::G55));
        assert_eq!(state.plane, Some(Plane::ZX));
        assert_eq!(state.units, Some(Units::Inch));
        assert_eq!(state.distance_mode, Some(DistanceMode::Incremental));
        assert_eq!(state.feed_rate_mode, Some(FeedRateMode::InverseTime));
        assert_eq!(state.spindle, Some(SpindleMode::CounterClockwise));
        assert_eq!(state.coolant, Some(CoolantMode::Mist));
        assert_eq!(state.tool, Some(5));
        assert_eq!(state.feed, Some(1.5));
        assert_eq!(state.spindle_rpm, Some(12000.0));
        assert_eq!(state.tool_length_offset.as_deref(), Some("G49"));
        assert_eq!(state.scaling, Some(true));
    }

    #[test]
    fn probe_motion_and_g59_3() {
        let state = parse_parser_state("[GC:G38.2 G59.3 G21 G90 M3 M8]").expect("valid");
        assert_eq!(state.motion.as_deref(), Some("G38.2"));
        assert_eq!(state.wcs, Some(Wcs::G59_3));
        assert_eq!(state.spindle, Some(SpindleMode::Clockwise));
        assert_eq!(state.coolant, Some(CoolantMode::Flood));
    }

    #[test]
    fn unknown_words_are_preserved() {
        let state = parse_parser_state("[GC:G0 G54 G1234 M999 Q7]").expect("valid");
        assert!(state.words.contains(&"G1234".to_string()));
        assert!(state.words.contains(&"M999".to_string()));
        assert!(state.words.contains(&"Q7".to_string()));
        assert_eq!(state.motion.as_deref(), Some("G0"));
        assert!(state.coolant.is_none());
    }

    #[test]
    fn malformed_or_empty_gc_is_not_fatal() {
        let state = parse_parser_state("[GC:]").expect("valid");
        assert!(state.words.is_empty());
        assert!(state.motion.is_none());
        let state = parse_parser_state("GC:G0 G21").expect("bare prefix");
        assert_eq!(state.units, Some(Units::Mm));
        assert!(parse_parser_state("G0 G54").is_err());
        assert!(parse_parser_state("").is_err());
    }

    #[test]
    fn non_ascii_words_never_panic() {
        let state = parse_parser_state("[GC:\u{e9}X G0 G21 M3]").expect("valid");
        assert_eq!(state.words.len(), 4);
        assert_eq!(state.units, Some(Units::Mm));
        assert_eq!(state.spindle, Some(SpindleMode::Clockwise));
        assert!(parse_parser_state("[GC:\u{1f600}]").is_ok());
    }

    #[test]
    fn bounded_word_count() {
        let words = (0..200).map(|_| "G0").collect::<Vec<_>>().join(" ");
        let state = parse_parser_state(&format!("[GC:{words}]")).expect("valid");
        assert_eq!(state.words.len(), MAX_WORDS);
    }
}
