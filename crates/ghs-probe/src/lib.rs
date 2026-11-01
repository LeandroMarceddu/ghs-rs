//! Pure probing plans and result math for the milling workflow.
//!
//! This crate builds bounded G-code plans, validates their safety parameters,
//! and computes offsets/centers/height maps from probe results. It performs no
//! I/O and never writes to a controller; the engine's job runner streams the
//! plans and the app owns preflight confirmation.
