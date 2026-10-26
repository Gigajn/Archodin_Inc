use archodin::graph::{
    AgentNode, AgentTool, ChainEvent, FallbackReason, Gemini, Graph, ModelChain, Provider,
    TypedTool,
};
use archodin::langchain_rust::language_models::llm::LLM;
use archodin::langchain_rust::language_models::LLMEvent;
use archodin::langchain_rust::schemas::Message;
use futures::StreamExt;
use mockito::{Matcher, Mock, Server, ServerGuard};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default, PartialEq)]
struct Job {
    request: String,
    answer: Option<Result<String, String>>,
}

#[derive(Deserialize, JsonSchema)]
struct AddInput {
    a: i64,
    b: i64,
    note: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("cannot add")]
struct AddError;

struct Add;

impl TypedTool for Add {
    type Input = AddInput;
    type Error = AddError;
    fn name(&self) -> String {
        "add".into()
    }
    fn description(&self) -> String {
        "Adds two integers".into()
    }
    async fn call(&self, input: AddInput) -> Result<String, AddError> {
        let _ = input.note;
        Ok((input.a + input.b).to_string())
    }
}

type Log = Arc<Mutex<Vec<ChainEvent>>>;

fn graph(chain: ModelChain, tools: Vec<AgentTool>) -> (Graph<Job>, Log) {
    let log: Log = Arc::default();
    let sink = log.clone();
    let mut agent = AgentNode::with_chain(chain, "Be brief").on_event(move |e| {
        sink.lock().unwrap().push(e);
    });
    for tool in tools {
        agent = agent.tool(tool);
    }
    let graph = Graph::builder()
        .agent_node(
            "agent",
            agent,
            |s: &Job| s.request.clone(),
            |mut s: Job, a| {
                s.answer = Some(a.map_err(|e| e.to_string()));
                s
            },
        )
        .entry("agent")
        .build()
        .unwrap();
    (graph, log)
}

fn job() -> Job {
    Job {
        request: "What is 2 + 40?".into(),
        answer: None,
    }
}

fn gemini_text(text: &str) -> String {
    json!({
        "candidates": [{"content": {"role": "model", "parts": [{"text": text}]},
            "finishReason": "STOP"}],
        "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 3, "totalTokenCount": 10}
    })
    .to_string()
}

fn gemini_call(name: &str, args: Value) -> String {
    json!({
        "candidates": [{"content": {"role": "model",
            "parts": [{"functionCall": {"name": name, "args": args}}]},
            "finishReason": "STOP"}]
    })
    .to_string()
}

const GEMINI_PATH: &str = "/v1beta/models/fake-gemini:generateContent";

async fn gemini_replying(status: usize, body: String) -> (ServerGuard, Mock) {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", GEMINI_PATH)
        .match_header("x-goog-api-key", "gkey")
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect_at_least(0)
        .create_async()
        .await;
    (server, mock)
}

fn gemini(server: &ServerGuard) -> Provider {
    Provider::gemini_at(server.url(), "gkey", "fake-gemini").account("gemini-acct")
}

fn openai_text(text: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text},
            "finish_reason": "stop"}]
    })
    .to_string()
}

#[tokio::test]
async fn a_gemini_provider_answers_a_chat() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", GEMINI_PATH)
        .match_header("x-goog-api-key", "gkey")
        .match_body(Matcher::PartialJson(json!({
            "systemInstruction": {"parts": [{"text": "Be brief"}]},
            "contents": [{"role": "user", "parts": [{"text": "What is 2 + 40?"}]}]
        })))
        .with_header("content-type", "application/json")
        .with_body(gemini_text("42"))
        .create_async()
        .await;
    let (graph, _) = graph(ModelChain::new(gemini(&server)), vec![]);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("42".into())));
    mock.assert_async().await;
}

#[tokio::test]
async fn a_gemini_provider_calls_a_tool_and_reads_its_result() {
    let mut server = Server::new_async().await;
    // The first request carries the Tool declaration and gets a function call.
    let first = server
        .mock("POST", GEMINI_PATH)
        .match_request(|request| {
            let body = String::from_utf8_lossy(request.body().unwrap()).to_string();
            let sent: Value = serde_json::from_str(&body).unwrap();
            let declaration = &sent["tools"][0]["functionDeclarations"][0];
            declaration["name"] == "add"
                && declaration["description"] == "Adds two integers"
                && !body.contains("functionResponse")
                // Gemini rejects these JSON Schema keywords.
                && !body.contains("additionalProperties")
                && !body.contains("$schema")
                && declaration["parameters"]["properties"]["a"]["type"] == "integer"
        })
        .with_header("content-type", "application/json")
        .with_body(gemini_call("add", json!({"a": 2, "b": 40})))
        .create_async()
        .await;
    // The second one returns the Tool result to the model, which then answers.
    let second = server
        .mock("POST", GEMINI_PATH)
        .match_body(Matcher::PartialJson(json!({
            "contents": [
                {"role": "user"},
                {"role": "model", "parts": [{"functionCall": {"name": "add",
                    "args": {"a": 2, "b": 40}}}]},
                {"role": "user", "parts": [{"functionResponse": {"name": "add",
                    "response": {"result": "42"}}}]}
            ]
        })))
        .with_header("content-type", "application/json")
        .with_body(gemini_text("The sum is 42"))
        .create_async()
        .await;
    let (graph, _) = graph(
        ModelChain::new(gemini(&server)),
        vec![AgentTool::typed(Add)],
    );
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("The sum is 42".into())));
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn a_gemini_provider_streams_text_then_the_complete_result() {
    let mut server = Server::new_async().await;
    let chunk = |parts: Value, usage: bool| {
        let mut v = json!({"candidates": [{"content": {"role": "model", "parts": parts}}]});
        if usage {
            v["usageMetadata"] =
                json!({"promptTokenCount": 5, "candidatesTokenCount": 2, "totalTokenCount": 7});
        }
        format!("data: {v}\r\n\r\n")
    };
    let body = [
        chunk(json!([{"text": "Hel"}]), false),
        chunk(json!([{"text": "lo"}]), false),
        chunk(
            json!([{"functionCall": {"name": "add", "args": {"a": 1, "b": 2}}}]),
            true,
        ),
    ]
    .concat();
    let mock = server
        .mock("POST", "/v1beta/models/fake-gemini:streamGenerateContent")
        .match_query(Matcher::UrlEncoded("alt".into(), "sse".into()))
        .match_header("x-goog-api-key", "gkey")
        .with_header("content-type", "text/event-stream")
        .with_body(body)
        .create_async()
        .await;
    let llm = Gemini::new("gkey", "fake-gemini").with_base_url(server.url());
    let messages = [Message::new_human_message("hi")];
    let mut events = llm.generate_stream(&messages);
    let mut text = Vec::new();
    let mut done = None;
    while let Some(event) = events.next().await {
        match event.unwrap() {
            LLMEvent::Text(t) => text.push(t),
            LLMEvent::Done(result) => done = Some(result),
            _ => {}
        }
    }
    mock.assert_async().await;
    assert_eq!(text, ["Hel", "lo"]);
    let done = done.expect("a Done event ends the stream");
    assert_eq!(done.generation, "Hello");
    assert_eq!(done.tool_calls.len(), 1);
    assert_eq!(done.tool_calls[0].name, "add");
    assert_eq!(
        done.tool_calls[0].arguments_value(),
        json!({"a": 1, "b": 2})
    );
    assert_eq!(done.tokens.unwrap().total_tokens, 7);
}

#[tokio::test]
async fn a_rate_limited_gemini_falls_back_to_an_openai_compatible_provider() {
    let (limited, _m1) = gemini_replying(
        429,
        json!({"error": {"code": 429, "message": "quota", "status": "RESOURCE_EXHAUSTED"}})
            .to_string(),
    )
    .await;
    let mut compat = Server::new_async().await;
    let _m2 = compat
        .mock("POST", "/chat/completions")
        .with_header("content-type", "application/json")
        .with_body(openai_text("from compat"))
        .create_async()
        .await;
    let chain = ModelChain::new(gemini(&limited))
        .then(Provider::openai_compatible(compat.url(), "key", "m").account("router"));
    let (graph, log) = graph(chain, vec![]);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("from compat".into())));
    let events = log.lock().unwrap().clone();
    let [ChainEvent::Fallback {
        failed,
        chosen,
        reason,
    }] = &events[..]
    else {
        panic!("expected one fallback, got {events:?}")
    };
    assert_eq!(failed.account(), "gemini-acct");
    assert_eq!(failed.model(), "fake-gemini");
    assert_eq!(chosen.account(), "router");
    assert_eq!(*reason, FallbackReason::RateLimited);
}

#[tokio::test]
async fn a_failing_openai_compatible_provider_falls_back_to_gemini() {
    let mut broken = Server::new_async().await;
    let _m1 = broken
        .mock("POST", "/chat/completions")
        .with_status(500)
        .with_body("{}")
        .create_async()
        .await;
    let (good, _m2) = gemini_replying(200, gemini_text("from gemini")).await;
    let chain =
        ModelChain::new(Provider::openai_compatible(broken.url(), "k", "m")).then(gemini(&good));
    let (graph, _) = graph(chain, vec![]);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("from gemini".into())));
}

#[tokio::test]
async fn a_gemini_error_reports_the_status_and_message() {
    let (server, _m) = gemini_replying(
        400,
        json!({"error": {"code": 400, "message": "bad request", "status": "INVALID_ARGUMENT"}})
            .to_string(),
    )
    .await;
    let (graph, _) = graph(ModelChain::new(gemini(&server)), vec![]);
    let out = graph.run(job()).await.unwrap();
    let Some(Err(text)) = out.answer else {
        panic!("expected an error")
    };
    assert!(
        text.contains("400") && text.contains("bad request"),
        "{text}"
    );
}

fn claude_body(text: &str) -> String {
    json!({"id": "m", "type": "message", "role": "assistant", "model": "fake-claude",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 3, "output_tokens": 2}})
    .to_string()
}

#[tokio::test]
async fn claude_is_a_step_in_a_model_chain() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/messages")
        .match_header("x-api-key", "ckey")
        .with_header("content-type", "application/json")
        .with_body(claude_body("from claude"))
        .create_async()
        .await;
    let mut broken = Server::new_async().await;
    let _m = broken
        .mock("POST", "/chat/completions")
        .with_status(503)
        .with_body("{}")
        .create_async()
        .await;
    let chain = ModelChain::new(Provider::openai_compatible(broken.url(), "k", "m"))
        .then(Provider::claude_at(server.url(), "ckey", "fake-claude").account("claude-acct"));
    let (graph, _) = graph(chain, vec![]);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("from claude".into())));
    mock.assert_async().await;
}

#[cfg(feature = "ollama")]
#[tokio::test]
async fn ollama_is_a_step_in_a_model_chain() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/api/chat")
        .with_header("content-type", "application/json")
        .with_body(
            json!({"model": "fake-llama", "created_at": "2024-01-01T00:00:00Z",
                "message": {"role": "assistant", "content": "from ollama"},
                "done": true})
            .to_string(),
        )
        .create_async()
        .await;
    let chain = ModelChain::new(Provider::ollama(server.url(), "fake-llama"));
    let (graph, _) = graph(chain, vec![]);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("from ollama".into())));
    mock.assert_async().await;
}

#[tokio::test]
async fn any_langchain_llm_can_be_a_native_provider() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/v1/messages")
        .with_header("content-type", "application/json")
        .with_body(claude_body("custom"))
        .create_async()
        .await;
    let llm = archodin::langchain_rust::llm::Claude::new()
        .with_api_key("k")
        .with_base_url(server.url());
    let (graph, _) = graph(
        ModelChain::new(Provider::from_llm("custom", "claude-ish", llm)),
        vec![],
    );
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("custom".into())));
}
