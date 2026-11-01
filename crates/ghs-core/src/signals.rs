//! Input signal state parsed from the grblHAL `Pn:` status field.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

use crate::axis::{Axes, Axis};

/// Bitmask of grblHAL input signals.
///
/// Bits 0..=8 are the axis limit switches (X, Y, Z, A, B, C, U, V, W). Bits
/// 9..=23 are the control signals in the order grblHAL writes them in `Pn:`.
/// The upper probe-state letters (`I`, `J`, `K`) are newer grblHAL extensions
/// where an upper-case letter means triggered and a lower-case letter means
/// not triggered; only the triggered state is stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Signals(u32);

impl Signals {
    /// No signal asserted.
    pub const NONE: Signals = Signals(0);

    /// X limit switch.
    pub const X_LIMIT: Signals = Signals(1 << 0);
    /// Y limit switch.
    pub const Y_LIMIT: Signals = Signals(1 << 1);
    /// Z limit switch.
    pub const Z_LIMIT: Signals = Signals(1 << 2);
    /// A limit switch.
    pub const A_LIMIT: Signals = Signals(1 << 3);
    /// B limit switch.
    pub const B_LIMIT: Signals = Signals(1 << 4);
    /// C limit switch.
    pub const C_LIMIT: Signals = Signals(1 << 5);
    /// U limit switch.
    pub const U_LIMIT: Signals = Signals(1 << 6);
    /// V limit switch.
    pub const V_LIMIT: Signals = Signals(1 << 7);
    /// W limit switch.
    pub const W_LIMIT: Signals = Signals(1 << 8);
    /// Reset switch (`R`).
    pub const RESET: Signals = Signals(1 << 9);
    /// Feed hold switch (`H`).
    pub const FEED_HOLD: Signals = Signals(1 << 10);
    /// Cycle start switch (`S`).
    pub const CYCLE_START: Signals = Signals(1 << 11);
    /// Safety door switch (`D`).
    pub const DOOR: Signals = Signals(1 << 12);
    /// Block delete switch (`L`).
    pub const BLOCK_DELETE: Signals = Signals(1 << 13);
    /// Optional program stop switch (`T`).
    pub const OPTIONAL_STOP: Signals = Signals(1 << 14);
    /// E-stop switch (`E`).
    pub const ESTOP: Signals = Signals(1 << 15);
    /// Probe disconnected (`O`).
    pub const PROBE_DISCONNECTED: Signals = Signals(1 << 16);
    /// Motor fault (`F`).
    pub const MOTOR_FAULT: Signals = Signals(1 << 17);
    /// Motor warning (`M`).
    pub const MOTOR_WARNING: Signals = Signals(1 << 18);
    /// Single-block switch (`Q`).
    pub const SINGLE_BLOCK: Signals = Signals(1 << 19);
    /// Probe triggered (`P`).
    pub const PROBE: Signals = Signals(1 << 20);
    /// Primary probe triggered (`I`, newer probe-state builds).
    pub const PROBE_PRIMARY: Signals = Signals(1 << 21);
    /// Tool setter triggered (`J`, newer probe-state builds).
    pub const TOOL_SETTER: Signals = Signals(1 << 22);
    /// Secondary probe triggered (`K`, newer probe-state builds).
    pub const PROBE_SECONDARY: Signals = Signals(1 << 23);

    /// Raw bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Build a mask from raw bits.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Signals(bits)
    }

    /// Returns `true` when no signal is asserted.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` when `other` is also asserted.
    #[must_use]
    pub const fn contains(self, other: Signals) -> bool {
        self.0 & other.0 == other.0
    }

    /// Add one signal.
    pub fn insert(&mut self, signal: Signals) {
        self.0 |= signal.0;
    }

    /// Union of two masks.
    #[must_use]
    pub const fn union(self, other: Signals) -> Self {
        Signals(self.0 | other.0)
    }

    /// Axis limit switches present in the mask.
    #[must_use]
    pub fn axis_limits(self) -> Axes {
        let mut axes = Axes::empty();
        for axis in Axis::ALL {
            if self.contains(Self::axis_limit(axis)) {
                axes.insert(axis);
            }
        }
        axes
    }

    /// Limit-switch signal for one axis.
    #[must_use]
    pub const fn axis_limit(axis: Axis) -> Signals {
        Signals(1 << (axis as u32))
    }

    /// Returns `true` when the limit switch for `axis` is asserted.
    #[must_use]
    pub const fn contains_axis_limit(self, axis: Axis) -> bool {
        self.0 & (1 << (axis as u32)) != 0
    }

    /// Parse a grblHAL `Pn:` signal string.
    ///
    /// Unknown characters are ignored so a newer controller never breaks the
    /// parser. Lower-case letters mean "not triggered" for the probe-state
    /// letters and are ignored.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut signals = Signals::NONE;
        for ch in text.chars() {
            if let Some(signal) = Self::from_letter(ch) {
                signals.insert(signal);
            }
        }
        signals
    }

    /// Map one `Pn:` letter to its signal, if it is a known letter.
    #[must_use]
    pub const fn from_letter(letter: char) -> Option<Signals> {
        let signal = match letter {
            'X' => Signals::X_LIMIT,
            'Y' => Signals::Y_LIMIT,
            'Z' => Signals::Z_LIMIT,
            'A' => Signals::A_LIMIT,
            'B' => Signals::B_LIMIT,
            'C' => Signals::C_LIMIT,
            'U' => Signals::U_LIMIT,
            'V' => Signals::V_LIMIT,
            'W' => Signals::W_LIMIT,
            'R' => Signals::RESET,
            'H' => Signals::FEED_HOLD,
            'S' => Signals::CYCLE_START,
            'D' => Signals::DOOR,
            'L' => Signals::BLOCK_DELETE,
            'T' => Signals::OPTIONAL_STOP,
            'E' => Signals::ESTOP,
            'O' => Signals::PROBE_DISCONNECTED,
            'F' => Signals::MOTOR_FAULT,
            'M' => Signals::MOTOR_WARNING,
            'Q' => Signals::SINGLE_BLOCK,
            'P' => Signals::PROBE,
            'I' => Signals::PROBE_PRIMARY,
            'J' => Signals::TOOL_SETTER,
            'K' => Signals::PROBE_SECONDARY,
            _ => return None,
        };
        Some(signal)
    }
}

/// Signals in the order grblHAL writes them in the `Pn:` element.
const SIGNAL_ORDER: [Signals; 15] = [
    Signals::RESET,
    Signals::FEED_HOLD,
    Signals::CYCLE_START,
    Signals::DOOR,
    Signals::BLOCK_DELETE,
    Signals::OPTIONAL_STOP,
    Signals::ESTOP,
    Signals::PROBE_DISCONNECTED,
    Signals::MOTOR_FAULT,
    Signals::MOTOR_WARNING,
    Signals::SINGLE_BLOCK,
    Signals::PROBE,
    Signals::PROBE_PRIMARY,
    Signals::TOOL_SETTER,
    Signals::PROBE_SECONDARY,
];

/// Letters matching [`SIGNAL_ORDER`].
const SIGNAL_ORDER_LETTERS: [char; 15] = [
    'R', 'H', 'S', 'D', 'L', 'T', 'E', 'O', 'F', 'M', 'Q', 'P', 'I', 'J', 'K',
];

impl fmt::Display for Signals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for axis in Axis::ALL {
            if self.contains_axis_limit(axis) {
                f.write_str(axis.as_str())?;
            }
        }
        for (signal, letter) in SIGNAL_ORDER.into_iter().zip(SIGNAL_ORDER_LETTERS) {
            if self.contains(signal) {
                write!(f, "{letter}")?;
            }
        }
        Ok(())
    }
}

impl BitOr for Signals {
    type Output = Signals;

    fn bitor(self, rhs: Signals) -> Signals {
        Signals(self.0 | rhs.0)
    }
}

impl BitOrAssign for Signals {
    fn bitor_assign(&mut self, rhs: Signals) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Signals {
    type Output = Signals;

    fn bitand(self, rhs: Signals) -> Signals {
        Signals(self.0 & rhs.0)
    }
}

impl BitAndAssign for Signals {
    fn bitand_assign(&mut self, rhs: Signals) {
        self.0 &= rhs.0;
    }
}

/// Auxiliary (non-signal) IO state.
///
/// Vector lengths are whatever the controller reported; values are stored
/// verbatim so a newer board never breaks decoding.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuxPins {
    /// Digital input levels, one per reported input.
    pub digital_in: Vec<bool>,
    /// Digital output levels, one per reported output.
    pub digital_out: Vec<bool>,
    /// Raw analog input values, one per reported input.
    pub analog_in: Vec<u16>,
    /// Raw analog output values, one per reported output.
    pub analog_out: Vec<u16>,
}

impl AuxPins {
    /// Returns `true` when no auxiliary IO has been reported.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.digital_in.is_empty()
            && self.digital_out.is_empty()
            && self.analog_in.is_empty()
            && self.analog_out.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reference_signals() {
        let signals = Signals::parse("HSDEP");
        assert!(signals.contains(Signals::FEED_HOLD));
        assert!(signals.contains(Signals::CYCLE_START));
        assert!(signals.contains(Signals::DOOR));
        assert!(signals.contains(Signals::ESTOP));
        assert!(signals.contains(Signals::PROBE));
        assert_eq!(signals.to_string(), "HSDEP");
    }

    #[test]
    fn parse_all_documented_letters() {
        let cases = [
            ('X', Signals::X_LIMIT),
            ('Y', Signals::Y_LIMIT),
            ('Z', Signals::Z_LIMIT),
            ('A', Signals::A_LIMIT),
            ('B', Signals::B_LIMIT),
            ('C', Signals::C_LIMIT),
            ('U', Signals::U_LIMIT),
            ('V', Signals::V_LIMIT),
            ('W', Signals::W_LIMIT),
            ('R', Signals::RESET),
            ('H', Signals::FEED_HOLD),
            ('S', Signals::CYCLE_START),
            ('D', Signals::DOOR),
            ('L', Signals::BLOCK_DELETE),
            ('T', Signals::OPTIONAL_STOP),
            ('E', Signals::ESTOP),
            ('O', Signals::PROBE_DISCONNECTED),
            ('F', Signals::MOTOR_FAULT),
            ('M', Signals::MOTOR_WARNING),
            ('Q', Signals::SINGLE_BLOCK),
            ('P', Signals::PROBE),
            ('I', Signals::PROBE_PRIMARY),
            ('J', Signals::TOOL_SETTER),
            ('K', Signals::PROBE_SECONDARY),
        ];
        for (letter, signal) in cases {
            assert_eq!(Signals::from_letter(letter), Some(signal));
            assert_eq!(Signals::parse(&letter.to_string()), signal);
        }
        assert_eq!(Signals::from_letter('\u{0}'), None);
        assert_eq!(Signals::parse(""), Signals::NONE);
        assert_eq!(
            Signals::parse("XYZ!!!"),
            Signals::X_LIMIT | Signals::Y_LIMIT | Signals::Z_LIMIT
        );
    }

    #[test]
    fn display_keeps_reference_order() {
        let signals = Signals::PROBE
            | Signals::ESTOP
            | Signals::DOOR
            | Signals::CYCLE_START
            | Signals::FEED_HOLD;
        assert_eq!(signals.to_string(), "HSDEP");
        let limits = Signals::X_LIMIT | Signals::Z_LIMIT | Signals::W_LIMIT;
        assert_eq!(limits.to_string(), "XZW");
        assert_eq!(limits.axis_limits().to_string(), "XZW");
        assert!(limits.contains_axis_limit(Axis::Z));
        assert!(!limits.contains_axis_limit(Axis::Y));
    }

    #[test]
    fn unknown_letters_are_ignored() {
        let signals = Signals::parse("X?Y!");
        assert_eq!(signals, Signals::X_LIMIT | Signals::Y_LIMIT);
        // Lower-case probe-state letters mean "not triggered".
        assert_eq!(Signals::parse("ijk"), Signals::NONE);
    }

    #[test]
    fn aux_pins_default_is_empty() {
        let pins = AuxPins::default();
        assert!(pins.is_empty());
    }
}
