//! Sans-IO grblHAL protocol implementation.
//!
//! The crate consumes bytes and emits typed events, and encodes typed commands
//! into bytes. It performs no I/O and owns no threads.

/// Version of the protocol crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
