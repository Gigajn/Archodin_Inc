//! Model-account metadata (ADR 0023) survives through the store: save it,
//! load it back, overwrite it, remove it — and the table never holds a key.
//!
//! The record has no key field at all, so "no key in the table" is a
//! compile-time property, not something this test has to police at runtime.

use archodin_store::{MemoryStore, ModelAccountRecord, ModelAccountStore, SqlStore};

fn record(name: &str) -> ModelAccountRecord {
    ModelAccountRecord {
        name: name.to_string(),
        kind: "openai_compatible".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        default_model: "m1".to_string(),
        budget_tokens: None,
        budget_cost: None,
    }
}

#[tokio::test]
async fn sql_store_model_accounts_roundtrip_overwrite_remove() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    assert!(store.load_model_accounts().await.unwrap().is_empty());

    store.save_model_account(&record("main")).await.unwrap();
    store.save_model_account(&record("spare")).await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0], record("main"));
    assert_eq!(loaded[1], record("spare"));

    // Saving the same name again replaces it.
    let mut updated = record("main");
    updated.default_model = "m2".to_string();
    store.save_model_account(&updated).await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].default_model, "m2");

    store.remove_model_account("main").await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded, vec![record("spare")]);

    // Removing a name that was never there is not an error.
    store.remove_model_account("ghost").await.unwrap();
    assert_eq!(store.load_model_accounts().await.unwrap().len(), 1);
}

#[tokio::test]
async fn memory_store_model_accounts_roundtrip_overwrite_remove() {
    let store = MemoryStore::new();
    assert!(store.load_model_accounts().await.unwrap().is_empty());

    store.save_model_account(&record("main")).await.unwrap();
    store.save_model_account(&record("spare")).await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded, vec![record("main"), record("spare")]);

    let mut updated = record("main");
    updated.kind = "gemini".to_string();
    store.save_model_account(&updated).await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].kind, "gemini");

    store.remove_model_account("spare").await.unwrap();
    store.remove_model_account("ghost").await.unwrap();
    assert_eq!(store.load_model_accounts().await.unwrap(), vec![updated]);
}
