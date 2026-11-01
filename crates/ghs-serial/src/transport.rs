//! Transport abstraction shared by the serial and in-process loopback links.

use std::time::Duration;

use crate::TransportError;

/// An event produced by a [`Transport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    /// Bytes received from the controller. Chunk boundaries are arbitrary.
    Bytes(Vec<u8>),
    /// A non-recoverable link error. The transport closes after this event.
    Error(String),
    /// The link is closed and no further events will follow.
    Closed,
}

/// A byte-oriented, bidirectional link to a controller.
///
/// Reads are delivered as [`TransportEvent`] values on a channel owned by the
/// transport. Sends are synchronous: [`Self::send`] returns only after every
/// byte has been handed to the operating system and flushed.
pub trait Transport: Send {
    /// Writes all bytes and flushes them to the link.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Closed`] when the link is no longer open, or a
    /// typed I/O error when the write fails.
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError>;

    /// Waits up to `timeout` for the next event.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Timeout`] when no event arrived in time. A
    /// link that is closed produces [`TransportEvent::Closed`] before the
    /// underlying channel disconnects.
    fn recv_timeout(&self, timeout: Duration) -> Result<TransportEvent, TransportError>;

    /// Closes the link.
    ///
    /// Calling this more than once is harmless. For [`crate::SerialTransport`]
    /// this waits briefly for the reader thread to stop.
    fn close(&mut self);

    /// Returns `true` while the link can carry bytes.
    fn is_open(&self) -> bool;
}
