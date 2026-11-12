//! Basileia — the proprietary application built on the MIT-licensed
//! Archodin library (ADR 0007).
//!
//! This crate exists so the company can actually run: it assembles the
//! Agora (`archodin-console`), the Graph (`archodin-graph`), the Roster and
//! the SqlStore (`archodin-store`) into one server binary. It is NOT a
//! library for reuse — see `LICENSE-BASILEIA` (all rights reserved) and the
//! crate README.
//!
//! The library half lives here only so the assembly is testable: the smoke
//! test (`tests/assembly_smoke.rs`) calls [`assemble`] without starting the
//! server. The binary itself (`src/main.rs`) is a thin wrapper: read config,
//! assemble, bind, serve.

pub mod assembly;
pub mod config;

pub use assembly::{assemble, App, CompanyState, Error};
pub use config::{Config, ConfigError, DbConfig, ModelAccountConfig};
