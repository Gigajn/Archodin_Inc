//! The behaviour suite against a SQLite file; each "restart" opens the file again.
#![cfg(feature = "sqlite")]

#[macro_use]
mod common;

use std::sync::Arc;

use archodin_store::SqlStore;
use common::opener;

suite!(
    {
        let dir = Arc::new(tempfile::tempdir().unwrap());
        opener(move || {
            let dir = dir.clone();
            async move {
                let path = dir.path().join("company.db");
                Arc::new(SqlStore::sqlite_file(&path).await.unwrap())
            }
        })
    },
    []
);
