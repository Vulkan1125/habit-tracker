//! The parts of `habit` that every binary shares: the data model, the stats,
//! the store, the palettes and the two optional modules.
//!
//! The TUI and the CLI live in the `habit` binary; `habit-report` and
//! `habit-notify` are the optional modules the user installs on demand.

pub mod config;
pub mod csv;
pub mod format;
pub mod model;
pub mod modules;
pub mod reminders;
pub mod report;
pub mod stats;
pub mod store;
pub mod theme;
