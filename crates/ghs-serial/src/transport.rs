//! Transport abstraction shared by the serial and in-process loopback links.

use crossbeam_channel::Receiver;

use crate::TransportError;

/// An event produced by a [`Transport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    /// Bytes received from the controller. Chunk boundaries are arbitrary.
    Bytes(Vec<u8>),
    /// A non-recoverable link error. The transport closes after this event.
    Error(String),
    /// The link is closed; treat this as the final event.
    Closed,
}

/// A byte-oriented, bidirectional link to a controller.
///
/// Reads are delivered as [`TransportEvent`] values on the receiver returned by
/// [`Self::events`]. Sends are synchronous: [`Self::send`] returns only after
/// every byte has been handed to the operating system and flushed.
pub trait Transport: Send {
    /// Writes all bytes and flushes them to the link.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Closed`] when the link is no longer open, or a
    /// typed I/O error when the write fails.
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError>;

    /// Returns the only receive path for this transport.
    ///
    /// The receiver stays valid for the lifetime of the transport and is meant
    /// for `crossbeam_channel::select!` loops:
    ///
    /// ```no_run
    /// # use ghs_serial::Transport;
    /// # fn example(transport: &mut impl Transport) {
    /// let events = transport.events();
    /// // crossbeam_channel::select! {
    /// //     recv(events) -> event => { /* handle TransportEvent */ }
    /// // }
    /// # let _ = events;
    /// # }
    /// ```
    ///
    /// A closed link publishes [`TransportEvent::Closed`] before the channel
    /// disconnects. The receiver lives as long as the transport: call
    /// [`Self::close`], drain through [`TransportEvent::Closed`], then drop the
    /// transport. Events are queued without bound, so drain them promptly. Use
    /// one consumer; if the receiver is cloned, do not also read the original.
    fn events(&self) -> &Receiver<TransportEvent>;

    /// Closes the link.
    ///
    /// Calling this more than once is harmless. For [`crate::SerialTransport`]
    /// this waits briefly for the reader thread to stop.
    fn close(&mut self);

    /// Returns `true` while the link can carry bytes.
    fn is_open(&self) -> bool;
}
