//! Configuration for the Basileia server, read from the environment.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `BASILEIA_BIND` | `127.0.0.1:8080` | Address (and port) the Agora listens on. |
//! | `BASILEIA_DB` | `basileia.db` | SQLite file for the SqlStore; `:memory:` selects an in-memory store (tests, throwaway runs). This crate only enables the `sqlite` feature of `archodin-store`, so any other value is treated as a SQLite file name — a `postgres://...` URL would NOT connect to Postgres, it would be created as a (useless) file of that name. |
//! | `BASILEIA_ADMIN_USER` | `admin` | Username for the Agora login. |
//! | `BASILEIA_ADMIN_PASSWORD` | *(required)* | Password for the Agora login. There is no default and no way to start without one: an open Agora is exactly KR-07, and this binary refuses to be that server. The plaintext password is only ever held in memory at startup; it is hashed with `hash_password` and never written to disk. |

use std::net::SocketAddr;
use std::path::PathBuf;

/// Where the SqlStore keeps the company's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbConfig {
    /// A SQLite file, created when missing.
    File(PathBuf),
    /// A private in-memory SQLite database (tests, throwaway runs).
    Memory,
}

/// Everything `assemble` needs that is not code.
///
/// `Debug` is implemented by hand (not derived): the admin password must
/// never appear in a debug log, a panic message, or an error report.
#[derive(Clone)]
pub struct Config {
    /// Address the Agora listens on.
    pub bind: SocketAddr,
    /// Where the SqlStore keeps the company's data.
    pub db: DbConfig,
    /// Username for the Agora login.
    pub admin_user: String,
    /// Password for the Agora login. Must not be empty.
    pub admin_password: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("db", &self.db)
            .field("admin_user", &self.admin_user)
            .field("admin_password", &"[redacted]")
            .finish()
    }
}

/// Why configuration failed.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// `BASILEIA_BIND` does not parse as a socket address.
    #[error("invalid bind address `{0}`: {1}")]
    BadBind(String, String),
    /// No admin password was configured.
    #[error("BASILEIA_ADMIN_PASSWORD is required: refusing to start without auth (see KR-07)")]
    MissingPassword,
    /// The admin username is empty.
    #[error("BASILEIA_ADMIN_USER must not be empty")]
    EmptyUser,
    /// Reading an environment variable failed.
    #[error("environment error: {0}")]
    Env(#[from] std::env::VarError),
}

impl Config {
    /// Reads the configuration from the environment (see the module docs).
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind = std::env::var("BASILEIA_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
        let bind: SocketAddr = bind
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::BadBind(bind, e.to_string()))?;

        let db = match std::env::var("BASILEIA_DB").as_deref() {
            Ok(":memory:") => DbConfig::Memory,
            Ok(path) => DbConfig::File(PathBuf::from(path)),
            Err(_) => DbConfig::File(PathBuf::from("basileia.db")),
        };

        let admin_user = std::env::var("BASILEIA_ADMIN_USER").unwrap_or_else(|_| "admin".into());
        if admin_user.trim().is_empty() {
            return Err(ConfigError::EmptyUser);
        }

        // A non-Unicode password is a different failure from a missing one:
        // report it as the environment error it is, not as "missing".
        let admin_password = match std::env::var("BASILEIA_ADMIN_PASSWORD") {
            Ok(password) => password,
            Err(std::env::VarError::NotPresent) => {
                return Err(ConfigError::MissingPassword);
            }
            Err(error) => return Err(ConfigError::Env(error)),
        };
        if admin_password.is_empty() {
            return Err(ConfigError::MissingPassword);
        }

        Ok(Self {
            bind,
            db,
            admin_user,
            admin_password,
        })
    }
}
