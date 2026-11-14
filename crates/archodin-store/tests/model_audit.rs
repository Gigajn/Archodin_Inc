//! The append-only audit log (ADR 0024): lifecycle events land newest
//! first, rows are never rewritten, and the record carries no key — the
//! "no key in the table" property is structural (there is no key field),
//! so this test polices ordering and append-only behaviour instead.

use archodin_store::{
    MemoryStore, ModelAccountRecord, ModelAccountStore, ModelAuditRecord, ModelAuditStore, SqlStore,
};

fn event(action: &str, account: &str, result: &str) -> ModelAuditRecord {
    ModelAuditRecord {
        seq: 0,
        at: 1_700_000_000,
        actor: "admin".to_string(),
        action: action.to_string(),
        kind: "openai_compatible".to_string(),
        account: account.to_string(),
        model: "m1".to_string(),
        result: result.to_string(),
    }
}

#[tokio::test]
async fn sql_store_audit_is_append_only_and_newest_first() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    assert!(store.list_audit(100).await.unwrap().is_empty());

    store
        .record_audit(&event("provision", "main", "ok"))
        .await
        .unwrap();
    store
        .record_audit(&event("rotate", "main", "ok"))
        .await
        .unwrap();
    store
        .record_audit(&event("health_check", "main", "auth_failed"))
        .await
        .unwrap();

    let listed = store.list_audit(100).await.unwrap();
    assert_eq!(listed.len(), 3);
    // Newest first, with store-assigned sequence numbers in order.
    assert_eq!(listed[0].action, "health_check");
    assert_eq!(listed[1].action, "rotate");
    assert_eq!(listed[2].action, "provision");
    assert!(listed[0].seq > listed[1].seq && listed[1].seq > listed[2].seq);

    // The limit binds: only the two newest come back.
    let listed = store.list_audit(2).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].action, "health_check");
    assert_eq!(listed[1].action, "rotate");
}

#[tokio::test]
async fn memory_store_audit_is_append_only_and_newest_first() {
    let store = MemoryStore::new();
    store
        .record_audit(&event("provision", "main", "ok"))
        .await
        .unwrap();
    store
        .record_audit(&event("revoke", "main", "ok"))
        .await
        .unwrap();

    let listed = store.list_audit(100).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].action, "revoke");
    assert_eq!(listed[1].action, "provision");
    assert!(listed[0].seq > listed[1].seq);
    assert_eq!(store.list_audit(1).await.unwrap().len(), 1);
}

#[tokio::test]
async fn sql_store_account_budget_roundtrips() {
    let store = SqlStore::sqlite_in_memory().await.unwrap();
    let mut record = ModelAccountRecord {
        name: "main".to_string(),
        kind: "openai_compatible".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        default_model: "m1".to_string(),
        budget_tokens: Some(10_000),
        budget_cost: Some(2.5),
    };
    store.save_model_account(&record).await.unwrap();
    let loaded = store.load_model_accounts().await.unwrap();
    assert_eq!(loaded, vec![record.clone()]);

    // Clearing the caps persists as NULLs, not zeroes.
    record.budget_tokens = None;
    record.budget_cost = None;
    store.save_model_account(&record).await.unwrap();
    assert_eq!(store.load_model_accounts().await.unwrap(), vec![record]);
}
