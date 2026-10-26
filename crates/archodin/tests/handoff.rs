use archodin::graph::{AgentNode, Briefing, Graph, Handoff, Provider, Turn};
use mockito::{Matcher, Server, ServerGuard};
use serde_json::json;

#[derive(Debug, Clone, Default)]
struct Job {
    conversation: Vec<Turn>,
    answer: Option<Result<String, String>>,
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

fn briefing() -> Briefing {
    Briefing::new("Ship the invoice report")
        .done_so_far("Collected March data")
        .decision("Use CSV output")
        .open_question("Which currency?")
        .related("docs/report-spec.md")
}

fn graph(server: &ServerGuard) -> Graph<Job> {
    let receiver = AgentNode::new(
        Provider::openai_compatible(server.url(), "key", "any-model"),
        "You are the receiver",
    );
    Graph::builder()
        .handoff_node(
            "receiver",
            receiver,
            |s: &Job| Handoff::new(briefing(), s.conversation.clone()),
            |mut s: Job, answer| {
                s.answer = Some(answer.map_err(|e| e.to_string()));
                s
            },
        )
        .entry("receiver")
        .build()
        .unwrap()
}

fn job() -> Job {
    Job {
        conversation: vec![
            Turn::new("Kofi", "SECRET-HISTORY-LINE-1"),
            Turn::new("Yuki", "SECRET-HISTORY-LINE-2"),
        ],
        answer: None,
    }
}

#[tokio::test]
async fn receiver_gets_all_five_sections_and_not_the_senders_history() {
    let mut server = Server::new_async().await;
    let leak = server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex("SECRET-HISTORY".into()))
        .with_body(completion(Some("leaked"), vec![]))
        .expect(0)
        .create_async()
        .await;
    let headings = [
        "Goal",
        "Done so far",
        "Decisions",
        "Open questions",
        "Related material",
    ]
    .iter()
    .map(|h| Matcher::Regex(format!("## {h}")));
    let contents = [
        "Ship the invoice report",
        "Collected March data",
        "Use CSV output",
        "Which currency",
        "report-spec.md",
    ]
    .iter()
    .map(|t| Matcher::Regex((*t).into()));
    let ok = server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::AllOf(headings.chain(contents).collect()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some("on it"), vec![]))
        .expect(1)
        .create_async()
        .await;
    let out = graph(&server).run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("on it".into())));
    ok.assert_async().await;
    leak.assert_async().await;
}

#[tokio::test]
async fn a_history_tool_returns_the_full_conversation_from_the_state() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""role":\s*"tool""#.into()))
        .match_body(Matcher::Regex("SECRET-HISTORY-LINE-1".into()))
        .match_body(Matcher::Regex("SECRET-HISTORY-LINE-2".into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some("read it all"), vec![]))
        .create_async()
        .await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex("conversation_history".into()))
        .match_body(Matcher::Regex("## Goal".into()))
        .with_header("content-type", "application/json")
        .with_body(completion(
            None,
            vec![json!({"id": "t1", "type": "function",
                "function": {"name": "conversation_history", "arguments": "{}"}})],
        ))
        .create_async()
        .await;
    let out = graph(&server).run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("read it all".into())));
}
