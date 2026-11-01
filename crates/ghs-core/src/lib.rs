//! Core domain types for ghs-rs.
//!
//! This crate is transport- and UI-agnostic. It models coordinates, machine
//! state, work coordinate systems, overrides, settings, tools, capabilities,
//! alarms, and errors.

/// Version of the core crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
