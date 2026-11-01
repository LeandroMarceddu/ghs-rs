//! Core domain types for ghs-rs.
//!
//! This crate is transport- and UI-agnostic. It models coordinates, machine
//! state, work coordinate systems, overrides, signals, settings, tools,
//! capabilities, alarms, and errors.
//!
//! All stored coordinates are millimetres. Inch mode is a display and protocol
//! concern handled by [`Units`] at the edges.
//!
//! ```
//! use ghs_core::{Axes, Position, Units, Axis};
//!
//! let position = Position::xyz(25.4, 0.0, -2.0);
//! assert_eq!(position.get(Axis::X), 25.4);
//! assert_eq!(position.axes(), Axes::xyz());
//! assert_eq!(position.to_units(Units::Inch).get(Axis::X), 1.0);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod axis;
mod codes;
mod info;
mod overrides;
mod position;
mod probe;
mod profile;
mod settings;
mod signals;
mod snapshot;
mod state;
mod tools;
mod units;
mod wcs;

pub use axis::{AXIS_COUNT, Axes, Axis, InvalidAxisLetter};
pub use codes::{AlarmCode, ErrorCode};
pub use info::{AuxIoCounts, ControllerInfo, parse_controller_info};
pub use overrides::{FeedSpindle, OVERRIDE_PERCENT_DEFAULT, Overrides};
pub use position::Position;
pub use probe::ProbeResult;
pub use profile::ControllerProfile;
pub use settings::{AxisValues, Setting, SettingValue};
pub use signals::{AuxPins, Signals};
pub use snapshot::{BufferState, JobInfo, JobState, MachineSnapshot};
pub use state::{MachineState, RunSubstate};
pub use tools::ToolEntry;
pub use units::Units;
pub use wcs::{CoordinateSystem, Wcs};

/// Derived controller capabilities, parsed from the `$I+` report.
///
/// The architecture calls this record `Capabilities`; it is the same value as
/// [`ControllerInfo`], which carries both identity and capability data. Product
/// code branches on its capability helpers, never on a board or profile name.
pub type Capabilities = ControllerInfo;

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
