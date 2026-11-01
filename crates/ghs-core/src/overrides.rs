//! Feed, rapid and spindle overrides, plus the live feed/spindle state.

use serde::{Deserialize, Serialize};

/// Default override percentage reported by grblHAL.
pub const OVERRIDE_PERCENT_DEFAULT: u16 = 100;

/// Feed, rapid and spindle override percentages.
///
/// Values are percentages as reported in the `Ov:` status field. The
/// controller enforces its own limits; the domain stores whatever was
/// reported so the UI can display it faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    /// Feed override percentage.
    pub feed: u16,
    /// Rapid override percentage.
    pub rapid: u16,
    /// Spindle speed override percentage.
    pub spindle: u16,
}

impl Overrides {
    /// All overrides at 100 %.
    pub const DEFAULT: Overrides = Overrides {
        feed: OVERRIDE_PERCENT_DEFAULT,
        rapid: OVERRIDE_PERCENT_DEFAULT,
        spindle: OVERRIDE_PERCENT_DEFAULT,
    };

    /// Build an override set from three percentages.
    #[must_use]
    pub const fn new(feed: u16, rapid: u16, spindle: u16) -> Self {
        Overrides {
            feed,
            rapid,
            spindle,
        }
    }

    /// Returns `true` when every override is at its default.
    #[must_use]
    pub const fn is_default(self) -> bool {
        self.feed == OVERRIDE_PERCENT_DEFAULT
            && self.rapid == OVERRIDE_PERCENT_DEFAULT
            && self.spindle == OVERRIDE_PERCENT_DEFAULT
    }

    /// Feed rate multiplier, for example `1.25` at 125 %.
    #[must_use]
    pub fn feed_factor(self) -> f64 {
        f64::from(self.feed) / 100.0
    }

    /// Rapid rate multiplier.
    #[must_use]
    pub fn rapid_factor(self) -> f64 {
        f64::from(self.rapid) / 100.0
    }

    /// Spindle speed multiplier.
    #[must_use]
    pub fn spindle_factor(self) -> f64 {
        f64::from(self.spindle) / 100.0
    }
}

impl Default for Overrides {
    fn default() -> Self {
        Overrides::DEFAULT
    }
}

/// Live feed rate and spindle state from the `FS`/`A` status fields.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FeedSpindle {
    /// Current feed rate in millimetres per minute.
    pub feed: f64,
    /// Current spindle speed in revolutions per minute.
    pub spindle_rpm: f64,
    /// Whether the spindle is running.
    pub spindle_on: bool,
    /// Spindle direction: `true` for clockwise (M3), `false` for
    /// counter-clockwise (M4).
    pub clockwise: bool,
}

impl Default for FeedSpindle {
    fn default() -> Self {
        FeedSpindle {
            feed: 0.0,
            spindle_rpm: 0.0,
            spindle_on: false,
            clockwise: true,
        }
    }
}

impl FeedSpindle {
    /// Build a feed/spindle state.
    #[must_use]
    pub const fn new(feed: f64, spindle_rpm: f64, spindle_on: bool, clockwise: bool) -> Self {
        FeedSpindle {
            feed,
            spindle_rpm,
            spindle_on,
            clockwise,
        }
    }

    /// Returns `true` when the spindle is running.
    #[must_use]
    pub const fn is_spindle_running(self) -> bool {
        self.spindle_on && self.spindle_rpm > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_default_and_factors() {
        let overrides = Overrides::default();
        assert_eq!(overrides, Overrides::DEFAULT);
        assert!(overrides.is_default());
        assert_eq!(overrides.feed_factor(), 1.0);

        let reduced = Overrides::new(50, 25, 120);
        assert!(!reduced.is_default());
        assert_eq!(reduced.feed_factor(), 0.5);
        assert_eq!(reduced.rapid_factor(), 0.25);
        assert_eq!(reduced.spindle_factor(), 1.2);
    }

    #[test]
    fn feed_spindle_defaults_to_stopped_clockwise() {
        let state = FeedSpindle::default();
        assert_eq!(state.feed, 0.0);
        assert_eq!(state.spindle_rpm, 0.0);
        assert!(!state.spindle_on);
        assert!(state.clockwise);
        assert!(!state.is_spindle_running());
        assert!(FeedSpindle::new(500.0, 12000.0, true, false).is_spindle_running());
    }
}
