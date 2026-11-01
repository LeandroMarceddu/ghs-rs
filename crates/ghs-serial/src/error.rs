//! Typed errors for the serial transport.

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

    /// DTR/RTS control was requested for a port with line control disabled.
    #[error("line control is disabled for this port")]
    LineControlDisabled,

    /// Internal state was poisoned by a panicking writer.
    #[error("transport state is poisoned: {0}")]
    Poisoned(&'static str),
}
