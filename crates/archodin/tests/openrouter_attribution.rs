//! OpenRouter attribution on the wire (ADR 0027): a Provider built
//! with `Provider::openrouter` sends `HTTP-Referer` +
//! `X-OpenRouter-Title` on every chat completion, and a Provider built
//! with `Provider::openai_compatible` sends neither — the attribution
//! must not leak to any other OpenAI-compatible vendor.

use archodin::graph::{AgentNode, Provider};
use mockito::{Matcher, Server};
use serde_json::json;

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

#[tokio::test]
async fn an_openrouter_provider_sends_the_attribution_headers_on_chat_completions() {
    // Expected values are the ADR 0027 §D2 literals (independent source
    // of truth), not values read back from the code under test.
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("http-referer", "https://github.com/Gigajn/Archodin_Inc")
        .match_header("x-openrouter-title", "Basileia")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(completion("hello"))
        .create_async()
        .await;
    let provider = Provider::openrouter(server.url(), "sk-canary-openrouter", "m");
    let agent = AgentNode::new(provider, "Be brief");
    assert_eq!(agent.answer("hi").await.unwrap(), "hello");
    mock.assert();
}

#[tokio::test]
async fn an_openai_compatible_provider_sends_no_attribution_headers() {
    // Negative test (ADR 0027 §D4): the same two headers must be
    // ABSENT for a plain `openai_compatible` Provider, so Basileia's
    // identity never leaks to another vendor.
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("http-referer", Matcher::Missing)
        .match_header("x-openrouter-title", Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(completion("hello"))
        .create_async()
        .await;
    let provider = Provider::openai_compatible(server.url(), "sk-canary", "m");
    let agent = AgentNode::new(provider, "Be brief");
    assert_eq!(agent.answer("hi").await.unwrap(), "hello");
    mock.assert();
}
