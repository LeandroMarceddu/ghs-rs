//! Serial transport for grblHAL controllers.
//!
//! A dedicated reader thread owns the serial port; the rest of the application
//! exchanges byte buffers and events over channels.

/// Version of the serial crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
