//! Session engine: owns the transport, drives the grblHAL protocol, and
//! exposes a typed command and telemetry API.
//!
//! The engine runs on its own thread. A [`EngineHandle`] is `Send` and gives a
//! UI three cheap paths:
//!
//! - [`EngineHandle::snapshot`] returns the latest [`MachineSnapshot`] as an
//!   `Arc`; poll it at frame time.
//! - [`EngineHandle::events`] returns a bounded receiver for the console,
//!   alarms, discovery results and settings.
//! - [`EngineHandle::send`] passes a typed [`Command`] through one safety
//!   gateway; the UI never writes bytes to the controller.
//!
//! ```
//! use ghs_engine::{Engine, EngineConfig, EngineEvent};
//!
//! let handle = Engine::spawn(EngineConfig::default()).expect("engine");
//! assert!(!handle.is_connected());
//! let _events = handle.events();
//! let _snapshot = handle.snapshot();
//! // EngineEvent is the type drained from the event receiver.
//! fn _uses_event(_: EngineEvent) {}
//! # handle.shutdown();
//! ```
//!
//! The M2 slice covers connect, handshake, live telemetry, realtime commands,
//! jog, MDI, overrides, home/unlock/reset/sleep, settings read/write with
//! read-back verification, logs and the safety gateway. Job streaming is M3.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod command;
mod config;
mod error;
mod event;
mod handle;
mod overrides;
mod runtime;
mod telemetry;

pub use command::{Command, ConnectTarget};
pub use config::EngineConfig;
pub use error::EngineError;
pub use event::{EngineEvent, LogLevel};
pub use handle::{Engine, EngineHandle};

// Re-export the domain and protocol types used in the public API so a caller
// can construct commands without depending on every crate directly.
pub use ghs_core::{
    AlarmCode, Axes, Axis, ControllerInfo, FeedSpindle, MachineSnapshot, MachineState, Overrides,
    Position, ProbeResult, Setting, SettingValue, Signals, Wcs,
};
pub use ghs_proto::{
    CodeTable, EncodeError, ExtendedCommand, JogCommand, JogMode, Offsets, ParserState,
    RealtimeCommand, SettingDescription, SettingGroup, StatusReport, StreamerError, SystemCommand,
};
pub use ghs_serial::{SerialConfig, Transport, TransportEvent};

/// Version of the engine crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
