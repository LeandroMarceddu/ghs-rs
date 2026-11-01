//! Engine configuration.
//!
//! Every timing value is explicit so tests can run a full handshake, status
//! poll loop and watchdog in milliseconds instead of seconds.

use std::time::Duration;

use crate::EngineError;

/// Default time spent listening for unsolicited controller output, in
/// milliseconds.
pub const DEFAULT_HANDSHAKE_LISTEN_MS: u64 = 450;

/// Default time to wait for a status report during the handshake, in
/// milliseconds.
pub const DEFAULT_HANDSHAKE_RESPONSE_MS: u64 = 250;

/// Default time to wait for one discovery command response, in milliseconds.
pub const DEFAULT_DISCOVERY_TIMEOUT_MS: u64 = 3_000;

/// Default status-poll interval while the machine is moving, in milliseconds.
pub const DEFAULT_STATUS_POLL_ACTIVE_MS: u64 = 100;

/// Default status-poll interval while the machine is idle, in milliseconds.
pub const DEFAULT_STATUS_POLL_IDLE_MS: u64 = 500;

/// Default interval at which a full `0x87` status report is requested, in
/// milliseconds.
pub const DEFAULT_FULL_STATUS_INTERVAL_MS: u64 = 5_000;

/// Default watchdog delay before a feed hold is sent, in milliseconds.
pub const DEFAULT_WATCHDOG_PAUSE_MS: u64 = 2_000;

/// Default watchdog delay before the link is declared lost, in milliseconds.
pub const DEFAULT_WATCHDOG_DISCONNECT_MS: u64 = 5_000;

/// Default engine tick interval, in milliseconds.
pub const DEFAULT_TICK_INTERVAL_MS: u64 = 10;

/// Default timeout for a command or connect round trip, in milliseconds.
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 1_000;

/// Default bounded event queue size.
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 1_024;

/// Default bounded command queue size.
pub const DEFAULT_COMMAND_QUEUE_CAPACITY: usize = 256;

/// Default receive buffer size used when the controller does not report one.
pub const DEFAULT_RX_BUFFER_SIZE: usize = 128;

/// Configuration for an engine session.
///
/// The defaults are tuned for a real controller. Tests override every timing
/// with short values, usually by starting from [`EngineConfig::default`] and
/// overriding fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    /// How long to listen for unsolicited controller output after the
    /// transport opens.
    ///
    /// The protocol recommends 400-500 ms. A zero duration skips the listen
    /// phase entirely.
    pub handshake_listen: Duration,
    /// How long to wait for a status report during the handshake.
    pub handshake_response: Duration,
    /// How long to wait for one discovery command (`$I+`, `$$`, `$#`, `$G`,
    /// `$EA`, `$EE`, `$ES`, `$EG`) to complete.
    ///
    /// This is separate from [`EngineConfig::handshake_response`] because a
    /// controller can spend a second or more serialising `$ES` or a large
    /// `$$` response, while the initial status probe must fail fast.
    pub discovery_timeout: Duration,
    /// Status-poll interval while the controller reports motion.
    pub status_poll_active: Duration,
    /// Status-poll interval while the controller is idle.
    pub status_poll_idle: Duration,
    /// How often a full status report (`0x87`) is requested to refresh fields
    /// that are normally only sent on change.
    pub full_status_interval: Duration,
    /// How long without a status report before a feed hold is sent during
    /// motion.
    pub watchdog_pause: Duration,
    /// How long without a status report before the link is declared lost.
    ///
    /// Must be greater than or equal to [`EngineConfig::watchdog_pause`].
    pub watchdog_disconnect: Duration,
    /// Engine tick interval for timers, polling and the watchdog.
    ///
    /// Keep this at or below half of the shortest poll interval.
    pub tick_interval: Duration,
    /// Maximum time a caller waits for the engine thread to accept or reject a
    /// command or a connect request.
    pub command_timeout: Duration,
    /// Capacity of the bounded telemetry/event queue.
    pub event_queue_capacity: usize,
    /// Capacity of the bounded command queue.
    pub command_queue_capacity: usize,
    /// Receive buffer size used before the controller reports one in `$I+`.
    pub default_rx_buffer: usize,
    /// Maximum length of one command line, in bytes, excluding the
    /// terminator.
    pub max_command_line: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            handshake_listen: Duration::from_millis(DEFAULT_HANDSHAKE_LISTEN_MS),
            handshake_response: Duration::from_millis(DEFAULT_HANDSHAKE_RESPONSE_MS),
            discovery_timeout: Duration::from_millis(DEFAULT_DISCOVERY_TIMEOUT_MS),
            status_poll_active: Duration::from_millis(DEFAULT_STATUS_POLL_ACTIVE_MS),
            status_poll_idle: Duration::from_millis(DEFAULT_STATUS_POLL_IDLE_MS),
            full_status_interval: Duration::from_millis(DEFAULT_FULL_STATUS_INTERVAL_MS),
            watchdog_pause: Duration::from_millis(DEFAULT_WATCHDOG_PAUSE_MS),
            watchdog_disconnect: Duration::from_millis(DEFAULT_WATCHDOG_DISCONNECT_MS),
            tick_interval: Duration::from_millis(DEFAULT_TICK_INTERVAL_MS),
            command_timeout: Duration::from_millis(DEFAULT_COMMAND_TIMEOUT_MS),
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            command_queue_capacity: DEFAULT_COMMAND_QUEUE_CAPACITY,
            default_rx_buffer: DEFAULT_RX_BUFFER_SIZE,
            max_command_line: ghs_proto::MAX_COMMAND_LINE,
        }
    }
}

impl EngineConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidConfig`] for a zero capacity, a zero
    /// interval where progress is required, or a watchdog whose pause delay
    /// exceeds its disconnect delay.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.event_queue_capacity == 0 {
            return Err(EngineError::InvalidConfig(
                "event_queue_capacity must be greater than zero".into(),
            ));
        }
        if self.command_queue_capacity == 0 {
            return Err(EngineError::InvalidConfig(
                "command_queue_capacity must be greater than zero".into(),
            ));
        }
        for (name, duration) in [
            ("handshake_response", self.handshake_response),
            ("discovery_timeout", self.discovery_timeout),
            ("status_poll_active", self.status_poll_active),
            ("status_poll_idle", self.status_poll_idle),
            ("full_status_interval", self.full_status_interval),
            ("watchdog_disconnect", self.watchdog_disconnect),
            ("tick_interval", self.tick_interval),
            ("command_timeout", self.command_timeout),
        ] {
            if duration.is_zero() {
                return Err(EngineError::InvalidConfig(format!(
                    "{name} must be greater than zero"
                )));
            }
        }
        if self.watchdog_pause > self.watchdog_disconnect {
            return Err(EngineError::InvalidConfig(
                "watchdog_pause must not exceed watchdog_disconnect".into(),
            ));
        }
        if self.default_rx_buffer < 2 {
            return Err(EngineError::InvalidConfig(
                "default_rx_buffer must be at least two bytes".into(),
            ));
        }
        if self.max_command_line < 2 {
            return Err(EngineError::InvalidConfig(
                "max_command_line must be at least two bytes".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = EngineConfig::default();
        config.validate().expect("defaults are valid");
        assert_eq!(config.status_poll_active.as_millis(), 100);
        assert_eq!(config.status_poll_idle.as_millis(), 500);
        assert!(config.discovery_timeout > config.handshake_response);
        assert!(config.watchdog_pause <= config.watchdog_disconnect);
        assert!(config.tick_interval < config.status_poll_active);
    }

    #[test]
    fn zero_capacities_are_rejected() {
        let config = EngineConfig {
            event_queue_capacity: 0,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
        let config = EngineConfig {
            command_queue_capacity: 0,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_timings_are_rejected() {
        let config = EngineConfig {
            watchdog_pause: Duration::from_secs(6),
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());

        let config = EngineConfig {
            tick_interval: Duration::ZERO,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());

        let config = EngineConfig {
            default_rx_buffer: 1,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
