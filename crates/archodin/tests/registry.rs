//! The Model Registry through its public API (ADR 0023): accounts, chains
//! per Role, overrides per Agent, the explicit global default, and the
//! resolution order — plus the canary rule (ADR 0025 §1) that no key value
//! ever appears in a Registry error.

use archodin::graph::{ModelAccount, ModelError, ModelKind, Registry, Vault};

fn work() -> ModelAccount {
    ModelAccount::new(
        "work",
        ModelKind::OpenAiCompatible,
        "https://api.example.com/v1",
        "model-one",
    )
}

fn spare() -> ModelAccount {
    ModelAccount::new(
        "spare",
        ModelKind::OpenAiCompatible,
        "https://api.example.com/v1",
        "model-two",
    )
}

#[test]
fn resolution_follows_override_then_role_then_global_default() {
    let mut registry = Registry::new();
    registry.add_account(work()).unwrap();
    registry.add_account(spare()).unwrap();
    registry
        .set_global_default(vec![("work".into(), "model-one".into())])
        .unwrap();
    registry
        .set_role_chain("writer", vec![("spare".into(), "model-two".into())])
        .unwrap();
    registry
        .set_agent_override("nova", vec![("work".into(), "model-one".into())])
        .unwrap();

    assert_eq!(
        registry.resolve("nova", "writer").unwrap()[0].0.name,
        "work"
    );
    assert_eq!(
        registry.resolve("other", "writer").unwrap()[0].0.name,
        "spare"
    );
    assert_eq!(
        registry.resolve("other", "unknown-role").unwrap()[0].0.name,
        "work"
    );
}

#[test]
fn resolution_with_nothing_configured_is_a_typed_error() {
    let registry = Registry::new();
    assert_eq!(
        registry.resolve("nova", "writer").unwrap_err(),
        ModelError::not_configured("nova", "writer")
    );
}

#[test]
fn registration_refuses_a_bad_base_url_and_an_unknown_chain_account() {
    let mut registry = Registry::new();
    let bad = ModelAccount::new("bad", ModelKind::OpenAiCompatible, "not a url", "m");
    assert!(matches!(
        registry.add_account(bad).unwrap_err(),
        ModelError::ModelNotConfigured { .. }
    ));
    assert!(matches!(
        registry
            .set_global_default(vec![("ghost".into(), "m".into())])
            .unwrap_err(),
        ModelError::ModelNotConfigured { .. }
    ));
}

#[test]
fn a_canary_key_never_appears_in_a_failed_resolve() {
    let canary = format!(
        "sk-canary-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock runs forward")
            .as_nanos()
    );
    // The fake key lives in the Vault under the account's secret name; the
    // Registry holds only that name, never the value.
    let vault = Vault::new([7; 32].into());
    vault.put("model/openai_compatible/work", &canary);
    let mut registry = Registry::new();
    registry.add_account(work()).unwrap();
    assert!(!format!("{registry:?}").contains(&canary));
    let err = registry.resolve("nova", "writer").unwrap_err();
    assert!(!err.to_string().contains(&canary));
    assert!(!format!("{err:?}").contains(&canary));
}
