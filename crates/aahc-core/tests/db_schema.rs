//! Seam: the data layer's public behaviour.
//!
//! Two independent sources of truth are used on purpose:
//!   * literal expectations written from the SRS (§5), so the tests cannot be
//!     satisfied by simply reading back whatever the implementation wrote;
//!   * the frozen Python implementation, used as a drift oracle.

use std::path::{Path, PathBuf};
use std::process::Command;

use aahc_core::db;
use sqlx::Row;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

const SPEC_TABLES: [&str; 6] = [
    "divisions",
    "employees",
    "router_api_pool",
    "skills_registry",
    "trading_ledger",
    "workflow_steps",
];

/// The 11 free-tier lanes, transcribed from the SRS/Phase 1 seed.
#[rustfmt::skip]
const SPEC_LANES: [(&str, &str, &str, &str, i64); 11] = [
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

async fn table_names(path: &Path) -> Vec<String> {
    let pool = db::open_pool(path).await.expect("open pool");
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("query tables");
    pool.close().await;
    rows.iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

async fn lane_rows(path: &Path) -> Vec<(String, String, String, String, i64)> {
    let pool = db::open_pool(path).await.expect("open pool");
    let rows = sqlx::query(
        "SELECT provider_name, cluster_type, model_identifier, env_key_name, rpm_limit \
         FROM router_api_pool ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("query lanes");
    pool.close().await;
    rows.iter()
        .map(|row| {
            (
                row.get::<String, _>("provider_name"),
                row.get::<String, _>("cluster_type"),
                row.get::<String, _>("model_identifier"),
                row.get::<String, _>("env_key_name"),
                row.get::<i64, _>("rpm_limit"),
            )
        })
        .collect()
}

#[tokio::test]
async fn init_creates_every_table_the_spec_requires() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("office.db");

    db::init_database(&path).await.expect("init");

    let mut names = table_names(&path).await;
    names.sort();
    let mut expected: Vec<String> = SPEC_TABLES.iter().map(|t| (*t).to_owned()).collect();
    expected.sort();
    assert_eq!(names, expected, "table set must match the SRS exactly");
}

#[tokio::test]
async fn employee_table_carries_the_spec_columns_and_check_constraints() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("office.db");
    db::init_database(&path).await.expect("init");

    let pool = db::open_pool(&path).await.expect("open pool");
    let columns: Vec<String> = sqlx::query("SELECT name FROM pragma_table_info('employees')")
        .fetch_all(&pool)
        .await
        .expect("columns")
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    for required in [
        "id",
        "division_id",
        "name",
        "role_title",
        "system_prompt",
        "model_cluster",
        "allowed_tools",
        "assigned_pair",
        "sprite_asset",
        "desk_coord_x",
        "desk_coord_y",
        "current_state",
    ] {
        assert!(
            columns.contains(&required.to_owned()),
            "employees missing {required}: {columns:?}"
        );
    }

    let ddl: String = sqlx::query("SELECT sql FROM sqlite_master WHERE name = 'employees'")
        .fetch_one(&pool)
        .await
        .expect("ddl")
        .get("sql");
    for cluster in [
        "CLUSTER_CODE",
        "CLUSTER_FAST",
        "CLUSTER_REASONING",
        "CLUSTER_VISION",
    ] {
        assert!(ddl.contains(cluster), "employees must constrain {cluster}");
    }
    pool.close().await;
}

#[tokio::test]
async fn router_pool_is_seeded_with_the_eleven_free_lanes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("office.db");

    db::init_database(&path).await.expect("init");

    let lanes = lane_rows(&path).await;
    let expected: Vec<(String, String, String, String, i64)> = SPEC_LANES
        .iter()
        .map(|(a, b, c, d, e)| {
            (
                (*a).to_owned(),
                (*b).to_owned(),
                (*c).to_owned(),
                (*d).to_owned(),
                *e,
            )
        })
        .collect();
    assert_eq!(lanes, expected);

    let clusters: Vec<(String, i64)> = {
        let pool = db::open_pool(&path).await.expect("open pool");
        let rows = sqlx::query(
            "SELECT cluster_type, COUNT(*) AS n FROM router_api_pool GROUP BY cluster_type ORDER BY cluster_type",
        )
        .fetch_all(&pool)
        .await
        .expect("clusters");
        pool.close().await;
        rows.iter()
            .map(|row| (row.get::<String, _>("cluster_type"), row.get::<i64, _>("n")))
            .collect()
    };
    assert_eq!(
        clusters,
        vec![
            ("CLUSTER_CODE".to_owned(), 3),
            ("CLUSTER_FAST".to_owned(), 3),
            ("CLUSTER_REASONING".to_owned(), 3),
            ("CLUSTER_VISION".to_owned(), 2),
        ]
    );
}

#[tokio::test]
async fn seeding_is_idempotent_across_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("office.db");

    let first = db::init_database(&path).await.expect("first init");
    let second = db::init_database(&path).await.expect("second init");

    assert_eq!(lane_rows(&path).await.len(), 11);
    assert_eq!(lane_rows(&path).await, lane_rows(&path).await);
    assert!(first.lanes_inserted >= 11, "first run seeds every lane");
    assert_eq!(second.lanes_inserted, 0, "second run must insert nothing");
    assert_eq!(second.lanes_total, 11);
}

#[tokio::test]
async fn unique_lane_index_prevents_duplicate_lanes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("office.db");
    db::init_database(&path).await.expect("init");

    let pool = db::open_pool(&path).await.expect("open pool");
    let indexes: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'router_api_pool' AND sql IS NOT NULL",
    )
    .fetch_all(&pool)
    .await
    .expect("indexes")
    .iter()
    .map(|row| row.get::<String, _>("name"))
    .collect();
    assert!(
        indexes.iter().any(|name| name == "idx_router_lane"),
        "expected the unique lane index, found {indexes:?}"
    );

    let duplicate = sqlx::query(
        "INSERT INTO router_api_pool (provider_name, cluster_type, model_identifier, env_key_name, rpm_limit) \
         VALUES ('groq_1', 'CLUSTER_FAST', 'llama-3.3-70b-versatile', 'GROQ_API_KEY_1', 30)",
    )
    .execute(&pool)
    .await;
    pool.close().await;
    assert!(duplicate.is_err(), "a duplicate lane must be rejected");
}

/// Drift oracle: the Rust schema must stay structurally identical to the frozen
/// Python implementation until ticket 17 removes it.
#[tokio::test]
async fn schema_matches_the_frozen_python_oracle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rust_db = dir.path().join("rust.db");
    let python_db = dir.path().join("python.db");

    db::init_database(&rust_db).await.expect("rust init");

    let init_script = repo_root().join("backend/init_db.py");
    assert!(
        init_script.is_file(),
        "the frozen Python oracle is missing at {} - it is required until ticket 17",
        init_script.display()
    );
    let output = Command::new("python3")
        .arg(&init_script)
        .arg("--db")
        .arg(&python_db)
        .output()
        .expect("run the frozen Python init");
    assert!(
        output.status.success(),
        "python oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rust_structure = structure_dump(&rust_db).await;
    let python_structure = structure_dump(&python_db).await;
    assert_eq!(
        rust_structure, python_structure,
        "Rust and Python schemas diverged:\nrust={rust_structure:#?}\npython={python_structure:#?}"
    );

    assert_eq!(
        lane_rows(&rust_db).await,
        lane_rows(&python_db).await,
        "seeded lanes must match the Python oracle"
    );
}

async fn structure_dump(path: &Path) -> Vec<String> {
    let pool = db::open_pool(path).await.expect("open pool");
    let mut dump = Vec::new();

    let tables: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("tables")
    .iter()
    .map(|row| row.get::<String, _>("name"))
    .collect();

    for table in tables {
        let columns = sqlx::query(&format!("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('{table}') ORDER BY cid"))
            .fetch_all(&pool)
            .await
            .expect("columns");
        for row in columns {
            dump.push(format!(
                "table {table} column {} type {} notnull {} default {:?} pk {}",
                row.get::<String, _>("name"),
                row.get::<String, _>("type"),
                row.get::<i64, _>("notnull"),
                row.get::<Option<String>, _>("dflt_value"),
                row.get::<i64, _>("pk"),
            ));
        }

        let foreign_keys = sqlx::query(&format!(
            "SELECT \"table\", \"from\", \"to\" FROM pragma_foreign_key_list('{table}') ORDER BY id"
        ))
        .fetch_all(&pool)
        .await
        .expect("fks");
        for row in foreign_keys {
            dump.push(format!(
                "table {table} fk {} -> {}.{}",
                row.get::<String, _>("from"),
                row.get::<String, _>("table"),
                row.get::<Option<String>, _>("to").unwrap_or_default(),
            ));
        }

        let indexes = sqlx::query(&format!(
            "SELECT name, \"unique\" FROM pragma_index_list('{table}') ORDER BY name"
        ))
        .fetch_all(&pool)
        .await
        .expect("indexes");
        for row in indexes {
            let name: String = row.get("name");
            let unique: i64 = row.get("unique");
            dump.push(format!("table {table} index {name} unique {unique}"));

            let columns = sqlx::query(&format!(
                "SELECT name FROM pragma_index_info('{name}') ORDER BY seqno"
            ))
            .fetch_all(&pool)
            .await
            .expect("index columns");
            for column in columns {
                dump.push(format!(
                    "table {table} index {name} column {}",
                    column.get::<String, _>("name")
                ));
            }
        }

        let ddl: String = sqlx::query("SELECT sql FROM sqlite_master WHERE name = ?")
            .bind(&table)
            .fetch_one(&pool)
            .await
            .expect("ddl")
            .get("sql");
        for marker in ["CHECK", "REFERENCES", "NOT NULL"] {
            dump.push(format!("table {table} ddl contains {marker}"));
        }
        dump.push(format!("table {table} ddl: {}", normalise(&ddl)));
    }

    pool.close().await;
    dump
}

fn normalise(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}
