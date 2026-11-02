//! Deterministic, sans-IO grblHAL simulator.
//!
//! One engine is parameterized by a [`ControllerProfile`]: `flexihal` is a
//! strict subset of `generic-full`, and the same code serves both. The
//! simulator owns no thread, socket, serial port, or wall clock, so a test is
//! fully reproducible.
//!
//! # Usage
//!
//! ```
//! use ghs_sim::{SimProfile, Simulator};
//!
//! let mut sim = Simulator::new(SimProfile::FlexiHal);
//! assert_eq!(sim.banner(), b"GrblHAL 1.1f ['$' for help]\r\n");
//!
//! // Feed a full status request, then parse the reply with ghs-proto.
//! let reply = sim.feed(b"$I+\r\n");
//! assert!(reply.starts_with(b"[VER:1.1f.20240604:]"));
//! assert!(reply.ends_with(b"ok\r\n"));
//! ```
//!
//! - [`Simulator::feed`] consumes arbitrary byte chunks (partial lines,
//!   `CR`/`LF`/`CRLF`, interleaved realtime bytes) and returns all output.
//! - [`Simulator::tick`] advances motion and timers and returns pending output.
//! - [`Simulator::snapshot`] exposes the controller state for assertions.
//! - [`Simulator::banner`] returns the welcome message; it is not queued
//!   automatically, so a loopback can choose when to "connect".
//!
//! # Capability data
//!
//! [`profile_flexihal`] reproduces the recorded `1.1f.20240604` identity byte
//! for byte. [`profile_generic_full`] is the superset used for broad scenario
//! coverage. A profile record can also be built from a recorded capture with
//! [`Simulator::from_profile`].
//!
//! # Deliberate limitations
//!
//! This is a protocol and engine test double, not a machine emulator:
//!
//! - Arc blocks move to their endpoint; no intermediate arc path is generated.
//! - `G38.x` probing completes successfully at its endpoint. The live probe
//!   input asserts on contact and opens on the next non-probe motion, while
//!   the last `[PRB]` result stays latched for `$#` until a reset or another
//!   probe.
//! - Stored startup lines are acknowledged as `>N<n>=...:ok` on reset but their
//!   G-code is not executed.
//! - `$REBOOT` performs a soft reset and prints the banner; it emits no `ok`.
//! - Feed and rapid overrides are reported but do not rescale motion timing.
//! - `$#` values are always millimetres; realtime status follows the active
//!   `G20`/`G21` unit mode.
//! - Positions start at the machine origin, and `$H` homes instantly rather
//!   than over several seconds.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod gcode;
mod motion;
mod profile;
mod settings;
mod simulator;
mod util;

pub use ghs_core::{ControllerInfo, ControllerProfile};
pub use profile::{MAX_SIM_SPINDLES, SimProfile, profile_flexihal, profile_generic_full};
pub use simulator::{SimSnapshot, Simulator};

/// Version of the simulator crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
