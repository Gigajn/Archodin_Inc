//! The behaviour suite against the in-memory backend.

#[macro_use]
mod common;

use std::sync::Arc;

use archodin_store::MemoryStore;
use common::opener;

suite!(
    {
        let store = Arc::new(MemoryStore::new());
        opener(move || {
            let store = store.clone();
            async move { store }
        })
    },
    []
);
