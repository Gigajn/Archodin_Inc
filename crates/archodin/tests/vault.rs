use archodin::graph::{AgentNode, ChainEvent, ModelChain, Provider, Vault, VaultError};
use mockito::Server;
use serde_json::json;

const KEY: [u8; 32] = [7; 32];

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

#[test]
fn secrets_are_resolved_by_name_and_stored_encrypted() {
    let vault = Vault::new(KEY);
    vault.put("groq", "sk-super-secret-123");
    assert_eq!(vault.names(), vec!["groq".to_string()]);
    assert_eq!(
        vault.resolve("groq").unwrap().expose(),
        "sk-super-secret-123"
    );
    assert_eq!(
        vault.resolve("nope").unwrap_err(),
        VaultError::UnknownSecret("nope".into())
    );

    let sealed = vault.seal();
    let text = String::from_utf8_lossy(&sealed);
    assert!(!text.contains("sk-super-secret-123"));
    assert!(text.contains("groq"));

    let reopened = Vault::unseal(KEY, &sealed).unwrap();
    assert_eq!(
        reopened.resolve("groq").unwrap().expose(),
        "sk-super-secret-123"
    );
    assert_eq!(
        Vault::unseal([8; 32], &sealed).unwrap_err(),
        VaultError::Decrypt
    );
}

#[test]
fn secrets_never_show_in_debug_output() {
    let vault = Vault::new(KEY);
    vault.put("groq", "sk-super-secret-123");
    let secret = vault.resolve("groq").unwrap();
    assert!(!format!("{secret:?}").contains("sk-super-secret-123"));
    assert!(!format!("{vault:?}").contains("sk-super-secret-123"));
    let provider = Provider::from_vault("http://x/v1", &vault, "groq", "m");
    assert!(!format!("{provider:?}").contains("sk-super-secret-123"));
    assert_eq!(
        vault.redact("failed with key sk-super-secret-123 today"),
        "failed with key [redacted] today"
    );
}

#[tokio::test]
async fn a_chain_step_resolves_its_key_at_call_time_and_rotation_applies_to_running_jobs() {
    let mut server = Server::new_async().await;
    let old = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer old-key")
        .with_header("content-type", "application/json")
        .with_body(completion("one"))
        .create_async()
        .await;
    let new = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer new-key")
        .with_header("content-type", "application/json")
        .with_body(completion("two"))
        .create_async()
        .await;

    let vault = Vault::new(KEY);
    vault.put("work", "old-key");
    let provider = Provider::from_vault(server.url(), &vault, "work", "m");
    let agent = AgentNode::new(provider, "Be brief");
    assert_eq!(agent.answer("hi").await.unwrap(), "one");

    vault.put("work", "new-key");
    assert_eq!(agent.answer("hi").await.unwrap(), "two");
    old.assert_async().await;
    new.assert_async().await;
}

#[tokio::test]
async fn a_missing_secret_falls_back_to_the_next_pair() {
    let mut server = Server::new_async().await;
    let ok = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer good")
        .with_header("content-type", "application/json")
        .with_body(completion("fine"))
        .create_async()
        .await;
    let vault = Vault::new(KEY);
    vault.put("second", "good");
    let chain = ModelChain::new(Provider::from_vault(server.url(), &vault, "gone", "m"))
        .then(Provider::from_vault(server.url(), &vault, "second", "m"));
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<ChainEvent>::new()));
    let sink = log.clone();
    let agent = AgentNode::with_chain(chain, "x").on_event(move |e| sink.lock().unwrap().push(e));
    assert_eq!(agent.answer("hi").await.unwrap(), "fine");
    assert_eq!(log.lock().unwrap().len(), 1);
    ok.assert_async().await;
}
