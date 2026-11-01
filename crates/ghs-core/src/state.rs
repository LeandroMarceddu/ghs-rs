//! Machine state and its optional substate.

use std::fmt;

use serde::{Deserialize, Serialize};

/// State of the grblHAL machine as reported in the first field of a realtime
/// status report.
///
/// [`MachineState::Unknown`] is the default so a snapshot that has never seen
/// telemetry never looks safe to move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MachineState {
    /// Idle and ready for a new command.
    Idle,
    /// Executing a program.
    Run,
    /// Feed hold, decelerating or fully stopped.
    Hold,
    /// Executing a jog command.
    Jog,
    /// Alarm; motion is locked until unlocked or reset.
    Alarm,
    /// Safety door state.
    Door,
    /// Check mode (`$C`); no motion is performed.
    Check,
    /// Homing cycle.
    Home,
    /// Sleep state.
    Sleep,
    /// Waiting for a manual tool change (grblHAL `Tool` state).
    Tool,
    /// Unrecognised state, never produced by a known grblHAL build.
    #[default]
    Unknown,
}

impl MachineState {
    /// Parse the state word from a status report, tolerating a numeric
    /// substate suffix such as `"Run:1"` or `"Alarm:3"`.
    ///
    /// Unknown words map to [`MachineState::Unknown`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self::parse_with_code(text).0
    }

    /// Parse a state word and its optional numeric substate.
    ///
    /// Returns the state and `None` when no substate suffix is present. A
    /// malformed suffix is treated as absent; the state itself is still
    /// resolved.
    #[must_use]
    pub fn parse_with_code(text: &str) -> (Self, Option<u16>) {
        let text = text.trim();
        let (word, code) = match text.split_once(':') {
            Some((word, code)) => (word.trim(), code.trim().parse::<u16>().ok()),
            None => (text, None),
        };
        (Self::from_word(word), code)
    }

    fn from_word(word: &str) -> Self {
        match word.to_ascii_lowercase().as_str() {
            "idle" => MachineState::Idle,
            "run" => MachineState::Run,
            "hold" => MachineState::Hold,
            "jog" => MachineState::Jog,
            "alarm" => MachineState::Alarm,
            "door" => MachineState::Door,
            "check" => MachineState::Check,
            "home" => MachineState::Home,
            "sleep" => MachineState::Sleep,
            "tool" => MachineState::Tool,
            _ => MachineState::Unknown,
        }
    }

    /// Canonical grblHAL state word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            MachineState::Idle => "Idle",
            MachineState::Run => "Run",
            MachineState::Hold => "Hold",
            MachineState::Jog => "Jog",
            MachineState::Alarm => "Alarm",
            MachineState::Door => "Door",
            MachineState::Check => "Check",
            MachineState::Home => "Home",
            MachineState::Sleep => "Sleep",
            MachineState::Tool => "Tool",
            MachineState::Unknown => "Unknown",
        }
    }

    /// Returns `true` when the controller is executing motion.
    #[must_use]
    pub const fn is_motion(self) -> bool {
        matches!(
            self,
            MachineState::Run | MachineState::Jog | MachineState::Home
        )
    }

    /// Returns `true` when the machine is locked out and requires an explicit
    /// unlock, homing or reset before motion is allowed.
    #[must_use]
    pub const fn is_locked(self) -> bool {
        matches!(
            self,
            MachineState::Alarm | MachineState::Door | MachineState::Unknown
        )
    }

    /// Returns `true` for states in which a new job may start.
    #[must_use]
    pub const fn can_start_job(self) -> bool {
        matches!(self, MachineState::Idle)
    }
}

impl From<&str> for MachineState {
    fn from(text: &str) -> Self {
        MachineState::parse(text)
    }
}

impl fmt::Display for MachineState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Optional substate reported after the machine state.
///
/// grblHAL documents `Run:1` as "feed hold pending" and `Run:2` as "probing".
/// Other numeric substates (`Hold`, `Door`, `Alarm`) are preserved separately
/// in the protocol status report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RunSubstate {
    /// No substate, or a substate that has no dedicated domain meaning.
    #[default]
    None,
    /// A feed hold was requested and its deceleration is pending.
    FeedHoldPending,
    /// A probing motion is in progress.
    Probing,
}

impl RunSubstate {
    /// Classify a reported substate.
    ///
    /// Only `Run:1` and `Run:2` carry domain meaning; every other combination
    /// maps to [`RunSubstate::None`].
    #[must_use]
    pub const fn from_report(state: MachineState, code: Option<u16>) -> Self {
        match (state, code) {
            (MachineState::Run, Some(1)) => RunSubstate::FeedHoldPending,
            (MachineState::Run, Some(2)) => RunSubstate::Probing,
            _ => RunSubstate::None,
        }
    }

    /// Short human-readable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            RunSubstate::None => "none",
            RunSubstate::FeedHoldPending => "feed hold pending",
            RunSubstate::Probing => "probing",
        }
    }
}

impl fmt::Display for RunSubstate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_every_state_word() {
        let cases = [
            ("Idle", MachineState::Idle, "Idle"),
            ("Run", MachineState::Run, "Run"),
            ("Hold", MachineState::Hold, "Hold"),
            ("Jog", MachineState::Jog, "Jog"),
            ("Alarm", MachineState::Alarm, "Alarm"),
            ("Door", MachineState::Door, "Door"),
            ("Check", MachineState::Check, "Check"),
            ("Home", MachineState::Home, "Home"),
            ("Sleep", MachineState::Sleep, "Sleep"),
            ("Tool", MachineState::Tool, "Tool"),
            ("Bogus", MachineState::Unknown, "Unknown"),
        ];
        for (text, expected, display) in cases {
            assert_eq!(MachineState::parse(text), expected);
            assert_eq!(expected.to_string(), display);
            assert_eq!(expected.as_str(), display);
        }
        assert_eq!(MachineState::parse("idle"), MachineState::Idle);
        assert_eq!(MachineState::parse(""), MachineState::Unknown);
    }

    #[test]
    fn parse_state_with_substate() {
        assert_eq!(
            MachineState::parse_with_code("Run:1"),
            (MachineState::Run, Some(1))
        );
        assert_eq!(
            MachineState::parse_with_code("Run:2"),
            (MachineState::Run, Some(2))
        );
        assert_eq!(
            MachineState::parse_with_code("Alarm:9"),
            (MachineState::Alarm, Some(9))
        );
        assert_eq!(
            MachineState::parse_with_code("Run:x"),
            (MachineState::Run, None)
        );
        assert_eq!(
            MachineState::parse_with_code("Run:"),
            (MachineState::Run, None)
        );
        assert_eq!(
            MachineState::parse_with_code("Run:99999"),
            (MachineState::Run, None)
        );
        assert_eq!(
            MachineState::parse_with_code("Idle"),
            (MachineState::Idle, None)
        );
    }

    #[test]
    fn substate_classification() {
        assert_eq!(
            RunSubstate::from_report(MachineState::Run, Some(1)),
            RunSubstate::FeedHoldPending
        );
        assert_eq!(
            RunSubstate::from_report(MachineState::Run, Some(2)),
            RunSubstate::Probing
        );
        assert_eq!(
            RunSubstate::from_report(MachineState::Hold, Some(1)),
            RunSubstate::None
        );
        assert_eq!(
            RunSubstate::from_report(MachineState::Run, None),
            RunSubstate::None
        );
        assert_eq!(RunSubstate::default(), RunSubstate::None);
    }

    #[test]
    fn lock_and_motion_predicates() {
        assert!(MachineState::Run.is_motion());
        assert!(MachineState::Home.is_motion());
        assert!(!MachineState::Hold.is_motion());
        assert!(MachineState::Alarm.is_locked());
        assert!(MachineState::Door.is_locked());
        assert!(MachineState::Unknown.is_locked());
        assert!(!MachineState::Idle.is_locked());
        assert!(MachineState::Idle.can_start_job());
        assert!(!MachineState::Run.can_start_job());
        assert_eq!(MachineState::default(), MachineState::Unknown);
    }
}
