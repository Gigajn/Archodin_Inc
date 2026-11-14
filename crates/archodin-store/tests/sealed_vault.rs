//! The sealed-vault envelope (ADR 0021) survives a restart through the store:
//! save it, load it back byte-identical, overwrite it, and start empty.

use archodin_graph::Vault;
use archodin_store::{MemoryStore, SealedVaultStore, SqlStore};

fn envelope_with(value: &str) -> Vec<u8> {
    let vault = Vault::new([7; 32].into());
    vault.put("model/openai/main", value);
    vault.seal_envelope().unwrap()
}

#[tokio::test]
async fn sql_store_sealed_vault_roundtrips_overwrites_and_starts_empty() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    assert_eq!(store.load_sealed_vault().await.unwrap(), None);

    let first = envelope_with("sk-canary-9f3a2b");
    store.save_sealed_vault(&first).await.unwrap();
    assert_eq!(
        store.load_sealed_vault().await.unwrap(),
        Some(first.clone())
    );

    let second = envelope_with("sk-canary-second");
    assert_ne!(first, second);
    store.save_sealed_vault(&second).await.unwrap();
    let loaded = store.load_sealed_vault().await.unwrap().unwrap();
    assert_eq!(loaded, second);

    // The stored bytes really open with the vault key.
    let reopened = Vault::unseal_envelope([7; 32].into(), &loaded).unwrap();
    assert_eq!(
        reopened.resolve("model/openai/main").unwrap().expose(),
        "sk-canary-second"
    );
}

#[tokio::test]
async fn sql_store_rejects_bytes_that_are_not_a_versioned_envelope() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    assert!(store.save_sealed_vault(b"not json").await.is_err());
    assert_eq!(store.load_sealed_vault().await.unwrap(), None);
}

#[tokio::test]
async fn memory_store_sealed_vault_roundtrips_overwrites_and_starts_empty() {
    let store = MemoryStore::new();
    assert_eq!(store.load_sealed_vault().await.unwrap(), None);

    let first = envelope_with("sk-canary-9f3a2b");
    store.save_sealed_vault(&first).await.unwrap();
    assert_eq!(
        store.load_sealed_vault().await.unwrap(),
        Some(first.clone())
    );

    let second = envelope_with("sk-canary-second");
    store.save_sealed_vault(&second).await.unwrap();
    assert_eq!(
        store.load_sealed_vault().await.unwrap(),
        Some(second.clone())
    );
}
