//! grblHAL simulator used by tests and by the application's demo mode.
//!
//! The simulator is deterministic and capability-configured. It can run
//! in-process over a byte-level loopback; it does not require a network stack.

/// Version of the simulator crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
