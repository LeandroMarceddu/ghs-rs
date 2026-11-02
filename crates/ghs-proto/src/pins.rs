//! Pin mapping, auxiliary pin state, limit switch and last-event reports.
//!
//! grblHAL reports these with `$PINS`, `$PINSTATE`, `$LIM` and `$LEV`. The
//! parsers accept letter masks (`XYZ`, `HSDEP`) and decimal or hexadecimal
//! numeric masks, ignore extra fields for forward compatibility, and reject
//! malformed payloads so the caller keeps the line as
//! [`crate::Response::Feedback`] instead of storing invented values.

use std::fmt;

use ghs_core::{Axes, Axis, Signals};

use crate::util::bounded;

/// Maximum number of input or output levels kept in a [`PinState`] record.
///
/// Real controllers report a handful of auxiliary pins. The bound protects the
/// parser from a malformed report that would otherwise allocate an unbounded
/// vector.
pub const MAX_PIN_STATE_BITS: usize = 256;

/// Maximum number of characters kept for one pin name.
const MAX_PIN_NAME_LEN: usize = 128;

/// Direction of a controller pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinDirection {
    /// The pin is an input.
    Input,
    /// The pin is an output.
    Output,
}

impl PinDirection {
    /// Parse the `input`/`output` token of a `[PIN:...]` line.
    ///
    /// The comparison is ASCII case-insensitive.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("input") {
            Some(PinDirection::Input)
        } else if text.eq_ignore_ascii_case("output") {
            Some(PinDirection::Output)
        } else {
            None
        }
    }

    /// Wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PinDirection::Input => "input",
            PinDirection::Output => "output",
        }
    }
}

impl fmt::Display for PinDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One `[PIN:<id>|<name>|<input|output>]` entry from `$PINS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinMapping {
    /// Controller pin number.
    pub id: u16,
    /// Pin function or description.
    pub name: String,
    /// Direction reported by the controller, when the token is known.
    ///
    /// An unknown or missing direction is `None`; the pin is still retained
    /// because a newer controller may use a token this build does not know.
    pub direction: Option<PinDirection>,
}

impl PinMapping {
    /// Parse the payload after `PIN:`.
    ///
    /// Returns `None` when the pin number is not a `u16` or the name is empty.
    /// An unknown direction token is preserved as `None`, and fields after the
    /// direction are ignored.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        let mut fields = payload.split('|');
        let id = fields.next()?.trim().parse::<u16>().ok()?;
        let name = bounded(fields.next()?, MAX_PIN_NAME_LEN);
        if name.is_empty() {
            return None;
        }
        let direction = fields.next().and_then(PinDirection::parse);
        Some(PinMapping {
            id,
            name,
            direction,
        })
    }
}

/// Auxiliary pin levels from `[PINSTATE:<mask>|<inputs>|<outputs>]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PinState {
    /// Raw signal mask reported by the controller.
    pub mask: Signals,
    /// Digital input levels, one per reported input, in report order.
    pub inputs: Vec<bool>,
    /// Digital output levels, one per reported output, in report order.
    pub outputs: Vec<bool>,
}

impl PinState {
    /// Parse the payload after `PINSTATE:`.
    ///
    /// The mask accepts the `Pn`-style letter form (`HSDEP`) or a decimal or
    /// `0x`-prefixed hexadecimal number. The input and output fields are
    /// `0`/`1` strings; a missing trailing field means no levels were reported.
    /// Extra fields are ignored.
    ///
    /// Returns `None` for an unknown mask character, a non-binary level
    /// character, or a level field above [`MAX_PIN_STATE_BITS`] entries.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        let mut fields = payload.split('|');
        let mask = parse_signal_mask(fields.next()?)?;
        let inputs = match fields.next() {
            Some(text) => parse_bit_string(text)?,
            None => Vec::new(),
        };
        let outputs = match fields.next() {
            Some(text) => parse_bit_string(text)?,
            None => Vec::new(),
        };
        Some(PinState {
            mask,
            inputs,
            outputs,
        })
    }
}

/// Limit-switch state from `[LIM:<mask>]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LimitState {
    /// Raw mask with one bit per axis in [`Axis::ALL`] order.
    pub mask: u16,
    /// Axes whose limit input is asserted.
    pub axes: Axes,
}

impl LimitState {
    /// Parse the payload after `LIM:`.
    ///
    /// Accepts a decimal or `0x`-prefixed hexadecimal axis mask, or the
    /// grblHAL axis-letter form such as `XZ`. Extra fields are ignored.
    ///
    /// Returns `None` for any other character or a number above `u16`.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        let mask = parse_axis_mask(payload.split('|').next()?.trim())?;
        Some(LimitState {
            mask,
            axes: Axes::from_bits_retain(mask),
        })
    }

    /// Returns `true` when the limit input for `axis` is asserted.
    #[must_use]
    pub const fn is_asserted(self, axis: Axis) -> bool {
        self.axes.contains(axis)
    }
}

/// Last control and limit events from `[LEV:<count>|<signals>]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LastEvents {
    /// Event counter reported by the controller.
    pub count: u32,
    /// Signals asserted in the last event.
    pub signals: Signals,
}

impl LastEvents {
    /// Parse the payload after `LEV:`.
    ///
    /// The signal field accepts the `Pn`-style letter form or a numeric mask.
    /// A missing signal field means no signals were reported. Extra fields are
    /// ignored.
    ///
    /// Returns `None` when the count is not a `u32` or the signal mask is
    /// malformed.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        let mut fields = payload.split('|');
        let count = fields.next()?.trim().parse::<u32>().ok()?;
        let signals = match fields.next() {
            Some(text) => parse_signal_mask(text)?,
            None => Signals::NONE,
        };
        Some(LastEvents { count, signals })
    }
}

/// Parse a `Pn`-style signal mask or a decimal/hexadecimal bitmask.
fn parse_signal_mask(text: &str) -> Option<Signals> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Signals::NONE);
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok().map(Signals::from_bits);
    }
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse::<u32>().ok().map(Signals::from_bits);
    }
    let mut signals = Signals::NONE;
    for character in text.chars() {
        signals.insert(Signals::from_letter(character)?);
    }
    Some(signals)
}

/// Parse an axis mask or a decimal/hexadecimal bitmask.
fn parse_axis_mask(text: &str) -> Option<u16> {
    if text.is_empty() {
        return Some(0);
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).ok();
    }
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse::<u16>().ok();
    }
    let mut mask = 0u16;
    for character in text.chars() {
        let axis = Axis::from_letter(character)?;
        mask |= 1 << axis.index();
    }
    Some(mask)
}

/// Parse a `0`/`1` string into levels, bounded by [`MAX_PIN_STATE_BITS`].
fn parse_bit_string(text: &str) -> Option<Vec<bool>> {
    let mut levels = Vec::with_capacity(text.len().min(MAX_PIN_STATE_BITS));
    for character in text.chars() {
        match character {
            '0' => levels.push(false),
            '1' => levels.push(true),
            ' ' | '\t' | ',' => continue,
            _ => return None,
        }
        if levels.len() > MAX_PIN_STATE_BITS {
            return None;
        }
    }
    Some(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_direction_is_case_insensitive() {
        assert_eq!(PinDirection::parse("input"), Some(PinDirection::Input));
        assert_eq!(PinDirection::parse(" OUTPUT "), Some(PinDirection::Output));
        assert_eq!(PinDirection::parse("analog"), None);
        assert_eq!(PinDirection::parse(""), None);
        assert_eq!(PinDirection::Input.to_string(), "input");
    }

    #[test]
    fn pin_mappings_keep_known_and_unknown_fields() {
        let mapping = PinMapping::parse("0|X limit|input").expect("valid");
        assert_eq!(
            mapping,
            PinMapping {
                id: 0,
                name: "X limit".into(),
                direction: Some(PinDirection::Input),
            }
        );

        let output = PinMapping::parse("12|Aux output 3|output|future|fields").expect("valid");
        assert_eq!(output.id, 12);
        assert_eq!(output.name, "Aux output 3");
        assert_eq!(output.direction, Some(PinDirection::Output));

        let unknown = PinMapping::parse("3|Mystery pin|analog").expect("valid");
        assert_eq!(unknown.direction, None);

        assert_eq!(
            PinMapping::parse("4|Input without a direction").expect("valid"),
            PinMapping {
                id: 4,
                name: "Input without a direction".into(),
                direction: None,
            }
        );
    }

    #[test]
    fn malformed_pin_mappings_are_rejected() {
        assert!(PinMapping::parse("map").is_none());
        assert!(PinMapping::parse("not-a-number|X limit|input").is_none());
        assert!(PinMapping::parse("0||input").is_none());
        assert!(PinMapping::parse("0|X limit").is_some());
        assert!(PinMapping::parse("70000|Too many|input").is_none());
        assert!(PinMapping::parse("|Name|input").is_none());
    }

    #[test]
    fn pin_state_parses_letters_and_bit_strings() {
        let state = PinState::parse("HSDEP|0101|1010").expect("valid");
        assert_eq!(state.mask, Signals::parse("HSDEP"));
        assert_eq!(state.inputs, vec![false, true, false, true]);
        assert_eq!(state.outputs, vec![true, false, true, false]);

        let numeric = PinState::parse("0x3|1|0").expect("valid");
        assert_eq!(numeric.mask, Signals::X_LIMIT | Signals::Y_LIMIT);
        assert_eq!(numeric.inputs, vec![true]);
        assert_eq!(numeric.outputs, vec![false]);

        let empty = PinState::parse("|").expect("valid");
        assert_eq!(empty.mask, Signals::NONE);
        assert!(empty.inputs.is_empty());
        assert!(empty.outputs.is_empty());
    }

    #[test]
    fn pin_state_ignores_extra_fields_and_rejects_malformed_levels() {
        let state = PinState::parse("0|01|10|future").expect("valid");
        assert_eq!(state.inputs, vec![false, true]);
        assert_eq!(state.outputs, vec![true, false]);

        assert!(PinState::parse("unknown|01|10").is_none());
        assert!(PinState::parse("0|012|10").is_none());
        assert!(PinState::parse("0|01|x").is_none());
        assert!(PinState::parse("0|01").is_some());
    }

    #[test]
    fn pin_state_levels_are_bounded() {
        let at_limit = "1".repeat(MAX_PIN_STATE_BITS);
        let state = PinState::parse(&format!("0|{at_limit}|")).expect("at the bound");
        assert_eq!(state.inputs.len(), MAX_PIN_STATE_BITS);

        let over_limit = "1".repeat(MAX_PIN_STATE_BITS + 1);
        assert!(PinState::parse(&format!("0|{over_limit}|")).is_none());
    }

    #[test]
    fn limit_state_parses_letters_numbers_and_extra_fields() {
        let letters = LimitState::parse("XZ").expect("valid");
        assert_eq!(letters.mask, 0b101);
        assert_eq!(letters.axes.to_string(), "XZ");
        assert!(letters.is_asserted(Axis::X));
        assert!(!letters.is_asserted(Axis::Y));
        assert!(letters.is_asserted(Axis::Z));

        let decimal = LimitState::parse("3|future").expect("valid");
        assert_eq!(decimal.axes.to_string(), "XY");

        let hex = LimitState::parse("0x4").expect("valid");
        assert_eq!(hex.axes.to_string(), "Z");

        let empty = LimitState::parse("").expect("valid");
        assert_eq!(empty.mask, 0);
        assert!(empty.axes.is_empty());
    }

    #[test]
    fn limit_state_rejects_non_axis_masks() {
        assert!(LimitState::parse("HSDEP").is_none());
        assert!(LimitState::parse("X!").is_none());
        assert!(LimitState::parse("70000").is_none());
        assert!(LimitState::parse("-1").is_none());
    }

    #[test]
    fn last_events_parse_count_and_signals() {
        let events = LastEvents::parse("17|HSDEP").expect("valid");
        assert_eq!(events.count, 17);
        assert_eq!(events.signals, Signals::parse("HSDEP"));

        let numeric = LastEvents::parse("3|0x1").expect("valid");
        assert_eq!(numeric.count, 3);
        assert_eq!(numeric.signals, Signals::X_LIMIT);

        let no_signals = LastEvents::parse("4").expect("valid");
        assert_eq!(no_signals.count, 4);
        assert_eq!(no_signals.signals, Signals::NONE);

        let extra = LastEvents::parse("5|X|future").expect("valid");
        assert_eq!(extra.signals, Signals::X_LIMIT);

        assert!(LastEvents::parse("many|X").is_none());
        assert!(LastEvents::parse("1|unknown").is_none());
        assert!(LastEvents::parse("-1|X").is_none());
    }

    #[test]
    fn malformed_payloads_never_panic() {
        let long = "1".repeat(MAX_PIN_STATE_BITS + 1);
        let payloads = [
            "",
            "|",
            "||",
            "|||",
            "0|",
            "0||",
            "not a number",
            "70000",
            "0x",
            "0xZZ",
            "?!",
            "\u{0}",
            "\u{1F600}",
            long.as_str(),
        ];
        for payload in payloads {
            let _ = PinMapping::parse(payload);
            let _ = PinState::parse(payload);
            let _ = LimitState::parse(payload);
            let _ = LastEvents::parse(payload);
        }
    }
}
