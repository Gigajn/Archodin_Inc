//! Data layer: SQLite schema creation and the free-tier router seed.
//!
//! The schema file is byte-identical to the frozen Python implementation
//! (`backend/schema.sql`) so the drift oracle in the tests can compare the two
//! structurally. It is executed directly (not through sqlx migrations) because
//! sqlx would add its own bookkeeping table, which the SRS schema does not have.

use std::path::Path;
use std::time::Duration;

use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use thiserror::Error;

/// Verbatim port of the Phase 1 DDL (7 tables incl. `skills_registry`).
pub const SCHEMA: &str = include_str!("../schema.sql");

/// Default zero-cost routing lanes. Order within a cluster is attempt priority.
#[rustfmt::skip]
pub const DEFAULT_LANES: [(&str, &str, &str, &str, i64); 11] = [
    ("openrouter_1", "CLUSTER_CODE",      "qwen/qwen-2.5-coder-32b-instruct:free", "OPENROUTER_API_KEY", 20),
    ("gemini_1",     "CLUSTER_CODE",      "gemini-2.5-flash",                      "GEMINI_API_KEY",     15),
    ("ollama_local", "CLUSTER_CODE",      "qwen2.5-coder:7b",                      "",                    0),
    ("groq_1",       "CLUSTER_FAST",      "llama-3.3-70b-versatile",               "GROQ_API_KEY_1",     30),
    ("cerebras_1",   "CLUSTER_FAST",      "llama-3.3-70b",                         "CEREBRAS_API_KEY",   30),
    ("groq_2",       "CLUSTER_FAST",      "llama-3.3-70b-versatile",               "GROQ_API_KEY_2",     30),
    ("openrouter_1", "CLUSTER_REASONING", "deepseek/deepseek-r1:free",             "OPENROUTER_API_KEY", 20),
    ("groq_1",       "CLUSTER_REASONING", "deepseek-r1-distill-llama-70b",         "GROQ_API_KEY_1",     30),
    ("gemini_1",     "CLUSTER_REASONING", "gemini-2.5-flash",                      "GEMINI_API_KEY",     15),
    ("gemini_1",     "CLUSTER_VISION",    "gemini-2.5-flash",                      "GEMINI_API_KEY",     15),
    ("ollama_local", "CLUSTER_VISION",    "llama3.2-vision:11b",                   "",                    0),
];

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("cannot prepare database directory {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSummary {
    pub tables: i64,
    pub lanes_inserted: i64,
    pub lanes_total: i64,
}

/// Open a pool with the pragmas the 24/7 office relies on (WAL, FK, busy timeout).
pub async fn open_pool(path: &Path) -> Result<SqlitePool, DbError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| DbError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    Ok(SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?)
}

/// Create/upgrade the schema and seed the free-tier pool. Idempotent.
pub async fn init_database(path: &Path) -> Result<InitSummary, DbError> {
    let pool = open_pool(path).await?;

    sqlx::raw_sql(SCHEMA).execute(&pool).await?;

    let mut lanes_inserted = 0;
    for (provider, cluster, model, env_key, rpm) in DEFAULT_LANES {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO router_api_pool \
             (provider_name, cluster_type, model_identifier, env_key_name, rpm_limit) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(provider)
        .bind(cluster)
        .bind(model)
        .bind(env_key)
        .bind(rpm)
        .execute(&pool)
        .await?;
        lanes_inserted += result.rows_affected() as i64;
    }

    let tables: i64 = sqlx::query(
        "SELECT COUNT(*) AS n FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await?
    .get("n");

    let lanes_total: i64 = sqlx::query("SELECT COUNT(*) AS n FROM router_api_pool")
        .fetch_one(&pool)
        .await?
        .get("n");

    pool.close().await;

    Ok(InitSummary {
        tables,
        lanes_inserted,
        lanes_total,
    })
}
