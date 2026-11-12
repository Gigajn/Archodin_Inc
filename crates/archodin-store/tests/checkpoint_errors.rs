//! KR-04: a CheckpointStore that cannot persist, or that holds corrupt data,
//! reports `Err` instead of panicking.

use archodin_graph::{Checkpoint, CheckpointStore};
#[cfg(feature = "sqlite")]
use archodin_store::SqlStore;
use archodin_store::{MemoryStore, StoreError};

/// A State whose serialization always fails.
#[derive(Debug, Clone, PartialEq)]
struct Unserializable;

impl serde::Serialize for Unserializable {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("boom: state cannot serialize"))
    }
}

impl<'de> serde::Deserialize<'de> for Unserializable {
    fn deserialize<D: serde::Deserializer<'de>>(_d: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("boom: state cannot deserialize"))
    }
}

fn checkpoint() -> Checkpoint<Unserializable> {
    Checkpoint {
        job: "job-1".into(),
        step: 0,
        state: Unserializable,
        next: None,
    }
}

#[tokio::test]
async fn memory_store_save_returns_err_on_unserializable_state() {
    let store = MemoryStore::new();
    let err = store
        .save(checkpoint())
        .await
        .expect_err("save of an unserializable State must return Err, not panic");
    assert!(matches!(err, StoreError::Invalid(_)));
}

#[tokio::test]
#[cfg(feature = "sqlite")]
async fn sql_store_save_returns_err_on_unserializable_state() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    let err = store
        .save(checkpoint())
        .await
        .expect_err("save of an unserializable State must return Err, not panic");
    assert!(matches!(err, StoreError::Invalid(_)));
}

#[tokio::test]
#[cfg(feature = "sqlite")]
async fn sql_store_checkpoints_returns_err_on_corrupt_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let store = SqlStore::sqlite_file(&path).await.unwrap();

    // Plant one corrupt checkpoint row with a second connection to the
    // same file, the way a damaged or hand-edited database would look.
    sqlx::any::install_default_drivers();
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO checkpoints (job, seq, step, state, next) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind("job-1")
    .bind(1i64)
    .bind(0i64)
    .bind("this is not json{")
    .bind(None::<String>)
    .execute(&pool)
    .await
    .unwrap();
    drop(pool);

    // The row is corrupt JSON, so deserializing it as any State fails;
    // `u32` is enough to pin the generic — the failure is in the data,
    // not the type. `S` lives on the `CheckpointStore` trait, not on the
    // method, so it is pinned with fully-qualified syntax.
    let err = CheckpointStore::<u32>::checkpoints(&store, "job-1")
        .await
        .expect_err("checkpoints over corrupt data must return Err, not panic");
    assert!(matches!(err, StoreError::Invalid(_)));
}

#[tokio::test]
async fn memory_store_save_and_checkpoints_round_trip() {
    // Happy path, kept so the new `Result` API is covered by a passing
    // round-trip, not only by its failure modes (the existing behaviour
    // suite also round-trips Checkpoints through both backends).
    let store = MemoryStore::new();
    store
        .save(Checkpoint {
            job: "job-1".into(),
            step: 0,
            state: 41u32,
            next: Some("next".into()),
        })
        .await
        .unwrap();
    let saved: Vec<Checkpoint<u32>> = store.checkpoints("job-1").await.unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].step, 0);
    assert_eq!(saved[0].state, 41);
    assert_eq!(saved[0].next.as_deref(), Some("next"));
}
