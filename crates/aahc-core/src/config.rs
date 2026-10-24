//! Configuration for the office service.

use std::net::SocketAddr;
use std::path::PathBuf;

use thiserror::Error;

pub const DB_PATH_VAR: &str = "AAHC_DB_PATH";
pub const BIND_ADDR_VAR: &str = "AAHC_BIND_ADDR";
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8000";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable: {name} (see .env.example)")]
    Missing { name: String },

    #[error("invalid value for {name}: {value:?} ({reason})")]
    Invalid {
        name: String,
        value: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub db_path: PathBuf,
    pub bind_addr: SocketAddr,
}

impl Config {
    /// Load configuration from an arbitrary lookup, so the seam is testable
    /// without mutating process environment.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let db_raw = get(DB_PATH_VAR)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ConfigError::Missing {
                name: DB_PATH_VAR.to_owned(),
            })?;

        let addr_raw = get(BIND_ADDR_VAR)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned());

        let bind_addr =
            addr_raw
                .trim()
                .parse::<SocketAddr>()
                .map_err(|error: std::net::AddrParseError| ConfigError::Invalid {
                    name: BIND_ADDR_VAR.to_owned(),
                    value: addr_raw.trim().to_owned(),
                    reason: error.to_string(),
                })?;

        Ok(Self {
            db_path: PathBuf::from(db_raw.trim()),
            bind_addr,
        })
    }

    /// Load configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }
}
