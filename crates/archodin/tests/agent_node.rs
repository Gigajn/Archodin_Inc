use std::sync::Arc;
use std::time::Duration;

use archodin::graph::{AgentNode, AgentTool, Graph, Provider, TypedTool};
use mockito::{Matcher, Server, ServerGuard};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Barrier;

#[derive(Debug, Clone, Default, PartialEq)]
struct Job {
    request: String,
    answer: Option<Result<String, String>>,
}

#[derive(Deserialize, JsonSchema)]
struct AddInput {
    a: i64,
    b: i64,
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
        Ok((input.a + input.b).to_string())
    }
}

/// Waits for its sibling before answering: only finishes if both run concurrently.
struct Rendezvous {
    name: &'static str,
    barrier: Arc<Barrier>,
}

#[derive(Deserialize, JsonSchema)]
struct Empty {}

impl TypedTool for Rendezvous {
    type Input = Empty;
    type Error = AddError;
    fn name(&self) -> String {
        self.name.into()
    }
    fn description(&self) -> String {
        "Meets its sibling".into()
    }
    async fn call(&self, _: Empty) -> Result<String, AddError> {
        self.barrier.wait().await;
        Ok(format!("{} done", self.name))
    }
}

fn tool_call(id: &str, name: &str, args: &str) -> serde_json::Value {
    json!({"id": id, "type": "function", "function": {"name": name, "arguments": args}})
}

fn completion(content: Option<&str>, calls: Vec<serde_json::Value>) -> String {
    let mut message = json!({"role": "assistant", "content": content});
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls);
    }
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": message,
            "finish_reason": if calls.is_empty() { "stop" } else { "tool_calls" }}]
    })
    .to_string()
}

/// A scripted OpenAI-compatible server: first turn asks for `calls`, and once
/// the tool results are in the conversation it answers with `final_answer`.
async fn scripted(
    calls: Vec<serde_json::Value>,
    final_answer: &str,
    expect_in_results: &str,
) -> ServerGuard {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""role":\s*"tool""#.into()))
        .match_body(Matcher::Regex(expect_in_results.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some(final_answer), vec![]))
        .create_async()
        .await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""name":\s*"(add|left|right)""#.into()))
        .match_body(Matcher::Regex("Be brief".into()))
        .with_header("content-type", "application/json")
        .with_body(completion(None, calls))
        .create_async()
        .await;
    server
}

fn graph_with(server: &ServerGuard, tools: Vec<AgentTool>) -> Graph<Job> {
    let provider = Provider::openai_compatible(server.url(), "key", "any-model");
    let mut agent = AgentNode::new(provider, "Be brief");
    for t in tools {
        agent = agent.tool(t);
    }
    Graph::builder()
        .agent_node(
            "agent",
            agent,
            |s: &Job| s.request.clone(),
            |mut s: Job, answer| {
                s.answer = Some(answer.map_err(|e| e.to_string()));
                s
            },
        )
        .entry("agent")
        .build()
        .unwrap()
}

fn job(request: &str) -> Job {
    Job {
        request: request.into(),
        answer: None,
    }
}

#[tokio::test]
async fn agent_node_answers_a_job_from_a_fake_provider() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex("Be brief".into()))
        .match_body(Matcher::Regex("Say hi".into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some("hi"), vec![]))
        .create_async()
        .await;
    let out = graph_with(&server, vec![])
        .run(job("Say hi"))
        .await
        .unwrap();
    assert_eq!(out.answer, Some(Ok("hi".into())));
}

#[tokio::test]
async fn agent_calls_a_typed_tool_and_uses_its_result() {
    let server = scripted(
        vec![tool_call("t1", "add", r#"{"a":2,"b":40}"#)],
        "the sum is 42",
        r"\b42\b",
    )
    .await;
    let out = graph_with(&server, vec![AgentTool::typed(Add)])
        .run(job("2+40?"))
        .await
        .unwrap();
    assert_eq!(out.answer, Some(Ok("the sum is 42".into())));
}

#[test]
fn typed_tool_schema_is_generated_from_the_input_type() {
    let tool = AgentTool::typed(Add);
    assert_eq!(tool.name(), "add");
    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["a"]["type"], "integer");
    assert!(schema["required"].as_array().unwrap().contains(&json!("b")));
}

#[tokio::test]
async fn two_independent_tool_calls_in_one_step_run_concurrently() {
    let barrier = Arc::new(Barrier::new(2));
    let server = scripted(
        vec![
            tool_call("t1", "left", "{}"),
            tool_call("t2", "right", "{}"),
        ],
        "both met",
        "left done",
    )
    .await;
    let tools = vec![
        AgentTool::typed(Rendezvous {
            name: "left",
            barrier: barrier.clone(),
        }),
        AgentTool::typed(Rendezvous {
            name: "right",
            barrier,
        }),
    ];
    let graph = graph_with(&server, tools);
    let out = tokio::time::timeout(Duration::from_secs(5), graph.run(job("go")))
        .await
        .expect("sequential tool calls would deadlock on the barrier")
        .unwrap();
    assert_eq!(out.answer, Some(Ok("both met".into())));
}

struct Shout;

#[async_trait::async_trait]
impl archodin::langchain_rust::tools::Tool for Shout {
    fn name(&self) -> String {
        "add".into()
    }
    fn description(&self) -> String {
        "Shouts".into()
    }
    async fn run(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!("SHOUT {}", input.as_str().unwrap_or_default()))
    }
}

#[tokio::test]
async fn an_existing_langchain_tool_works_through_the_adapter() {
    let server = scripted(
        vec![tool_call("t1", "add", r#"{"input":"hey"}"#)],
        "done",
        "SHOUT hey",
    )
    .await;
    let out = graph_with(&server, vec![AgentTool::from_langchain(Arc::new(Shout))])
        .run(job("shout"))
        .await
        .unwrap();
    assert_eq!(out.answer, Some(Ok("done".into())));
}

#[tokio::test]
async fn a_provider_error_reaches_the_state_writer() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .with_status(500)
        .with_body("{}")
        .create_async()
        .await;
    let out = graph_with(&server, vec![]).run(job("x")).await.unwrap();
    assert!(matches!(out.answer, Some(Err(_))));
}
