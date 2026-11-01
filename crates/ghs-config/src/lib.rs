//! Local configuration, profiles, tool tables, and backups.
//!
//! This crate owns data at rest, not controller I/O. The engine writes settings
//! and offsets; this crate stores the user's profiles, app preferences, tool
//! table, and versioned backups with atomic writes and rolling backup files.
