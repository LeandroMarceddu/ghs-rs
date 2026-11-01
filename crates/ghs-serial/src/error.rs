//! Typed errors for the serial transport.

use std::time::Duration;

/// Errors produced while enumerating ports or using a [`crate::Transport`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// The operating system rejected a serial port operation.
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    /// A byte read or write failed.
    #[error("serial I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The configuration was rejected before the port was opened.
    #[error("invalid serial configuration: {0}")]
    InvalidConfig(String),

    /// The transport is closed and cannot send more bytes.
    #[error("transport is closed")]
    Closed,

    /// No event arrived within the requested timeout.
    #[error("timed out after {0:?} waiting for transport data")]
    Timeout(Duration),

    /// Internal state was poisoned by a panicking writer.
    #[error("transport state is poisoned: {0}")]
    Poisoned(&'static str),
}

impl TransportError {
    /// Returns `true` when this error only means that no event arrived in time.
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }
}
