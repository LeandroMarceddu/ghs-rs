//! Serial transport for grblHAL controllers.
//!
//! A dedicated reader thread owns a cloned serial handle; the rest of the
//! application exchanges byte buffers and events over channels. The crate also
//! provides an in-process loopback pair for tests and simulator demo mode; it
//! never opens a network transport.
//!
//! # Receive path
//!
//! [`SerialTransport::open`] returns the transport, and
//! [`Transport::events`] returns the one event receiver. A reader thread
//! publishes [`TransportEvent::Bytes`] for every chunk read from the port,
//! [`TransportEvent::Error`] for a link error, and exactly one
//! [`TransportEvent::Closed`] when the link stops. The receiver is suitable for
//! `crossbeam_channel::select!`:
//!
//! ```no_run
//! # use ghs_serial::{SerialConfig, SerialTransport, Transport};
//! # fn example(config: SerialConfig) -> Result<(), Box<dyn std::error::Error>> {
//! let transport = SerialTransport::open(config)?;
//! let events = transport.events();
//! // crossbeam_channel::select! {
//! //     recv(events) -> event => { /* handle TransportEvent */ }
//! // }
//! # let _ = events;
//! # Ok(())
//! # }
//! ```
//!
//! # Loopback
//!
//! ```
//! use std::time::Duration;
//!
//! use ghs_serial::{Transport, TransportEvent, loopback_pair};
//!
//! let (mut sender, receiver) = loopback_pair();
//! sender.send(b"$I+\r\n").expect("send");
//! let event = receiver
//!     .events()
//!     .recv_timeout(Duration::from_secs(1))
//!     .expect("event");
//! assert_eq!(event, TransportEvent::Bytes(b"$I+\r\n".to_vec()));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod error;
mod loopback;
mod ports;
mod serial;
mod transport;

pub use config::{DataBits, FlowControl, Parity, SerialConfig, StopBits};
pub use error::TransportError;
pub use loopback::{LoopbackTransport, loopback_pair};
pub use ports::{PortInfo, PortWatcher, list_ports, watch_ports};
pub use serial::SerialTransport;
pub use transport::{Transport, TransportEvent};

/// Version of the serial crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
