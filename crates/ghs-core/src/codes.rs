//! Alarm and error codes with best-effort descriptions.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A grblHAL alarm code, the `<n>` in `ALARM:<n>`.
///
/// Codes come from grblHAL `alarms.h` and the legacy Grbl 1.1 set. The
/// description table is best-effort and version-dependent: newer firmware can
/// add codes, so unknown values must still be displayed safely.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct AlarmCode(pub u16);

impl AlarmCode {
    /// No alarm.
    pub const NONE: AlarmCode = AlarmCode(0);
    /// Hard limit triggered.
    pub const HARD_LIMIT: AlarmCode = AlarmCode(1);
    /// Soft limit exceeded.
    pub const SOFT_LIMIT: AlarmCode = AlarmCode(2);
    /// Emergency stop.
    pub const ESTOP: AlarmCode = AlarmCode(10);
    /// Motor fault.
    pub const MOTOR_FAULT: AlarmCode = AlarmCode(17);
    /// I/O expander exception.
    pub const EXPANDER_EXCEPTION: AlarmCode = AlarmCode(20);

    /// Returns `true` for the critical alarms that lock the controller.
    ///
    /// The current grblHAL headers treat alarms 1 (hard limit), 2 (soft limit),
    /// 10 (E-stop), 17 (motor fault) and 20 (I/O expander exception) as
    /// critical: only real-time commands are accepted until the condition
    /// clears and the user unlocks, homes or resets.
    #[must_use]
    pub const fn is_critical(self) -> bool {
        matches!(self.0, 1 | 2 | 10 | 17 | 20)
    }

    /// Build a code from its numeric value.
    #[must_use]
    pub const fn new(code: u16) -> Self {
        AlarmCode(code)
    }

    /// Numeric code.
    #[must_use]
    pub const fn code(self) -> u16 {
        self.0
    }

    /// Returns `true` for code 0, which means "no alarm".
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` when a description is known for this code.
    #[must_use]
    pub const fn is_known(self) -> bool {
        !self.description().is_empty()
    }

    /// Best-effort English description, empty for unknown codes.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self.0 {
            1 => "hard limit triggered",
            2 => "soft limit exceeded",
            3 => "reset while in motion",
            4 => "probe not in the expected initial state",
            5 => "probe did not contact the workpiece",
            6 => "homing cycle was reset",
            7 => "safety door opened during homing",
            8 => "homing pull-off failed to clear the limit switch",
            9 => "homing could not find the limit switch",
            10 => "emergency stop",
            11 => "homing required",
            12 => "limit switch engaged",
            13 => "probe protection triggered",
            14 => "spindle fault",
            15 => "auto-squaring approach failed",
            16 => "power-on self-test failed",
            17 => "motor fault",
            18 => "homing configuration error",
            19 => "Modbus exception",
            20 => "I/O expander communication failed",
            21 => "non-volatile storage failure",
            _ => "",
        }
    }
}

impl From<u16> for AlarmCode {
    fn from(code: u16) -> Self {
        AlarmCode(code)
    }
}

impl From<AlarmCode> for u16 {
    fn from(code: AlarmCode) -> Self {
        code.0
    }
}

impl fmt::Display for AlarmCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            f.write_str("no alarm")
        } else if self.is_known() {
            write!(f, "alarm {}: {}", self.0, self.description())
        } else {
            write!(f, "alarm {}", self.0)
        }
    }
}

/// A grblHAL error code, the `<n>` in `error:<n>`.
///
/// Codes come from grbl 1.1 and the grblHAL `errors.c` table. The description
/// table is best-effort and version-dependent; unknown values display as
/// `error N`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ErrorCode(pub u16);

impl ErrorCode {
    /// No error / `ok`.
    pub const OK: ErrorCode = ErrorCode(0);
    /// The command requires the idle state.
    pub const NOT_IDLE: ErrorCode = ErrorCode(8);
    /// Unsupported or invalid g-code command.
    pub const UNSUPPORTED_COMMAND: ErrorCode = ErrorCode(20);
    /// A limit switch is engaged.
    pub const LIMIT_ENGAGED: ErrorCode = ErrorCode(45);
    /// Homing is required before motion.
    pub const HOMING_REQUIRED: ErrorCode = ErrorCode(46);
    /// Emergency stop is active.
    pub const ESTOP: ErrorCode = ErrorCode(50);

    /// Build a code from its numeric value.
    #[must_use]
    pub const fn new(code: u16) -> Self {
        ErrorCode(code)
    }

    /// Numeric code.
    #[must_use]
    pub const fn code(self) -> u16 {
        self.0
    }

    /// Returns `true` for code 0, which means "ok".
    #[must_use]
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` when a description is known for this code.
    #[must_use]
    pub const fn is_known(self) -> bool {
        !self.description().is_empty()
    }

    /// Best-effort English description, empty for unknown codes.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self.0 {
            1 => "expected a command letter",
            2 => "bad number format",
            3 => "unsupported system command",
            4 => "negative value for an expected positive value",
            5 => "homing is not enabled",
            6 => "step pulse time must be at least 3 microseconds",
            7 => "non-volatile storage read failed; defaults restored",
            8 => "command requires the idle state",
            9 => "g-code commands are locked out in alarm or jog",
            10 => "soft limits require homing",
            11 => "maximum line length exceeded",
            12 => "setting would exceed the maximum step rate",
            13 => "safety door opened",
            14 => "line exceeds the storage length limit",
            15 => "jog target exceeds machine travel",
            16 => "invalid jog command",
            17 => "laser mode requires PWM output",
            18 => "reset asserted",
            19 => "value must be positive",
            20 => "unsupported or invalid g-code command",
            21 => "modal group violation",
            22 => "feed rate is undefined",
            23 => "value must be an integer",
            24 => "conflicting axis words",
            25 => "repeated g-code word",
            26 => "missing axis words",
            27 => "invalid line number",
            28 => "missing value word",
            29 => "G59.x work coordinate systems are not supported",
            30 => "G53 requires G0 or G1",
            31 => "unexpected axis words",
            32 => "arc requires an in-plane axis word",
            33 => "invalid motion target",
            34 => "invalid arc radius",
            35 => "arc requires in-plane offset words",
            36 => "unused value words",
            37 => "G43.1 offset is not on the tool length axis",
            38 => "tool number out of range",
            39 => "value out of range",
            40 => "command not allowed while a tool change is pending",
            41 => "spindle is not running when motion was commanded",
            42 => "threading requires the ZX plane",
            43 => "maximum feed rate exceeded",
            44 => "spindle RPM out of range",
            45 => "limit switch engaged",
            46 => "homing required",
            47 => "current tool is not set",
            48 => "conflicting value word",
            50 => "emergency stop active",
            60 => "SD card mount failed",
            61 => "SD card open or read failed",
            62 => "SD card directory listing failed",
            63 => "SD card directory not found",
            64 => "SD card file is empty",
            70 => "Bluetooth initialisation failed",
            _ => "",
        }
    }
}

impl From<u16> for ErrorCode {
    fn from(code: u16) -> Self {
        ErrorCode(code)
    }
}

impl From<ErrorCode> for u16 {
    fn from(code: ErrorCode) -> Self {
        code.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            f.write_str("ok")
        } else if self.is_known() {
            write!(f, "error {}: {}", self.0, self.description())
        } else {
            write!(f, "error {}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_alarms_include_descriptions() {
        assert_eq!(AlarmCode::new(1).description(), "hard limit triggered");
        assert_eq!(
            AlarmCode::new(21).description(),
            "non-volatile storage failure"
        );
        assert_eq!(AlarmCode::HARD_LIMIT.code(), 1);
        assert!(AlarmCode::ESTOP.is_known());
        assert_eq!(
            AlarmCode::new(1).to_string(),
            "alarm 1: hard limit triggered"
        );
    }

    #[test]
    fn unknown_alarms_never_panic() {
        for code in [0, 22, 99, 255, 999, u16::MAX] {
            let alarm = AlarmCode::new(code);
            let text = alarm.to_string();
            assert!(!text.is_empty());
            if code == 0 {
                assert_eq!(text, "no alarm");
            } else if !alarm.is_known() {
                assert_eq!(text, format!("alarm {code}"));
            }
        }
        assert!(!AlarmCode::new(99).is_known());
    }

    #[test]
    fn known_errors_include_descriptions() {
        assert_eq!(
            ErrorCode::new(20).description(),
            "unsupported or invalid g-code command"
        );
        assert_eq!(ErrorCode::NOT_IDLE.code(), 8);
        assert_eq!(
            ErrorCode::new(8).to_string(),
            "error 8: command requires the idle state"
        );
        assert_eq!(ErrorCode::OK.to_string(), "ok");
    }

    #[test]
    fn unknown_errors_never_panic() {
        for code in [0, 49, 99, 255, 999, u16::MAX] {
            let error = ErrorCode::new(code);
            let text = error.to_string();
            assert!(!text.is_empty());
            if code == 0 {
                assert_eq!(text, "ok");
            } else if !error.is_known() {
                assert_eq!(text, format!("error {code}"));
            }
        }
        assert!(!ErrorCode::new(99).is_known());
    }

    #[test]
    fn critical_alarms_are_flagged() {
        for code in [1, 2, 10, 17, 20] {
            assert!(AlarmCode::new(code).is_critical(), "alarm {code}");
        }
        for code in [0, 3, 9, 11, 13, 21, 99] {
            assert!(!AlarmCode::new(code).is_critical(), "alarm {code}");
        }
    }

    #[test]
    fn conversions() {
        assert_eq!(AlarmCode::from(3u16), AlarmCode::new(3));
        assert_eq!(u16::from(AlarmCode::new(3)), 3);
        assert_eq!(ErrorCode::from(3u16), ErrorCode::new(3));
        assert_eq!(u16::from(ErrorCode::new(3)), 3);
        assert_eq!(AlarmCode::default(), AlarmCode::NONE);
        assert_eq!(ErrorCode::default(), ErrorCode::OK);
    }
}
