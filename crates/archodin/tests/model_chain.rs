use std::sync::{Arc, Mutex};
use std::time::Duration;

use archodin::graph::{
    AgentNode, ChainEvent, FallbackReason, Graph, ModelChain, Provider, Role, Roster,
};
use mockito::{Server, ServerGuard};
use serde_json::json;

#[derive(Debug, Clone, Default, PartialEq)]
struct Job {
    request: String,
    answer: Option<Result<String, String>>,
}

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

fn rate_limit_body() -> String {
    json!({"error": {"message": "slow down", "type": "rate_limit", "code": "rate_limit"}})
        .to_string()
}

async fn replying(status: usize, body: String) -> (ServerGuard, mockito::Mock) {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect_at_least(0)
        .create_async()
        .await;
    (server, mock)
}

fn account(server: &ServerGuard, name: &str, model: &str) -> Provider {
    Provider::openai_compatible(server.url(), "key", model).account(name)
}

type Log = Arc<Mutex<Vec<ChainEvent>>>;

fn run_chain(chain: ModelChain) -> (Graph<Job>, Log) {
    let log: Log = Arc::default();
    let sink = log.clone();
    let agent = AgentNode::with_chain(chain, "Be brief").on_event(move |e| {
        sink.lock().unwrap().push(e);
    });
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
        request: "hi".into(),
        answer: None,
    }
}

#[tokio::test]
async fn an_error_moves_the_agent_to_the_next_pair_and_reports_it() {
    let (broken, _m1) = replying(500, "{}".into()).await;
    let (good, _m2) = replying(200, completion("from second")).await;
    let chain =
        ModelChain::new(account(&broken, "acct-a", "m1")).then(account(&good, "acct-b", "m2"));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("from second".into())));
    let events = log.lock().unwrap().clone();
    let [ChainEvent::Fallback {
        failed,
        chosen,
        reason,
    }] = &events[..]
    else {
        panic!("expected one fallback, got {events:?}")
    };
    assert_eq!((failed.account(), failed.model()), ("acct-a", "m1"));
    assert_eq!((chosen.account(), chosen.model()), ("acct-b", "m2"));
    assert!(matches!(reason, FallbackReason::Error(_)));
}

#[tokio::test]
async fn a_429_is_reported_as_a_rate_limit() {
    let (limited, _m1) = replying(429, rate_limit_body()).await;
    let (good, _m2) = replying(200, completion("ok")).await;
    let chain = ModelChain::new(account(&limited, "a", "m")).then(account(&good, "b", "m"));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("ok".into())));
    let events = log.lock().unwrap().clone();
    assert!(matches!(
        &events[..],
        [ChainEvent::Fallback {
            reason: FallbackReason::RateLimited,
            ..
        }]
    ));
}

#[tokio::test]
async fn a_timeout_moves_to_the_next_pair() {
    // Accepts connections and never answers.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", silent.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            if let Ok((s, _)) = silent.accept().await {
                held.push(s);
            }
        }
    });
    let (good, _m) = replying(200, completion("late but fine")).await;
    let chain = ModelChain::new(Provider::openai_compatible(url, "key", "slow").account("slow"))
        .then(account(&good, "b", "m"))
        .timeout(Duration::from_millis(200));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("late but fine".into())));
    let events = log.lock().unwrap().clone();
    assert!(matches!(
        &events[..],
        [ChainEvent::Fallback {
            reason: FallbackReason::Timeout,
            ..
        }]
    ));
}

#[tokio::test]
async fn a_poor_answer_never_triggers_a_fallback() {
    let (first, m1) = replying(200, completion("")).await;
    let (second, m2) = replying(200, completion("better")).await;
    let m1 = m1.expect(1);
    let m2 = m2.expect(0);
    let chain = ModelChain::new(account(&first, "a", "m")).then(account(&second, "b", "m"));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok(String::new())));
    assert!(log.lock().unwrap().is_empty());
    m1.assert_async().await;
    m2.assert_async().await;
}

#[tokio::test]
async fn two_accounts_of_one_provider_can_share_a_chain() {
    // Same base URL (same Provider), two keys: the first key is rate limited.
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer key-one")
        .with_status(429)
        .with_body(rate_limit_body())
        .create_async()
        .await;
    server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer key-two")
        .with_header("content-type", "application/json")
        .with_body(completion("via key two"))
        .create_async()
        .await;
    let chain = ModelChain::new(
        Provider::openai_compatible(server.url(), "key-one", "gpt").account("personal"),
    )
    .then(Provider::openai_compatible(server.url(), "key-two", "gpt").account("work"));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert_eq!(out.answer, Some(Ok("via key two".into())));
    let events = log.lock().unwrap().clone();
    let [ChainEvent::Fallback { failed, chosen, .. }] = &events[..] else {
        panic!("expected one fallback, got {events:?}")
    };
    assert_eq!((failed.account(), chosen.account()), ("personal", "work"));
}

#[tokio::test]
async fn when_every_pair_fails_the_error_reaches_the_state_writer() {
    let (a, _m1) = replying(500, "{}".into()).await;
    let (b, _m2) = replying(500, "{}".into()).await;
    let chain = ModelChain::new(account(&a, "a", "m")).then(account(&b, "b", "m"));
    let (graph, log) = run_chain(chain);
    let out = graph.run(job()).await.unwrap();
    assert!(matches!(out.answer, Some(Err(_))));
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_hired_agent_falls_back_along_its_roles_chain() {
    let (broken, _m1) = replying(500, "{}".into()).await;
    let (good, _m2) = replying(200, completion("role fallback")).await;
    let roster = Roster::new();
    roster.add_role(
        Role::new("writer", "Be brief")
            .model_chain(vec![account(&broken, "a", "m1"), account(&good, "b", "m2")]),
    );
    let agent = roster.hire("writer", "Ines").unwrap();
    let log: Log = Arc::default();
    let sink = log.clone();
    agent.on_event(move |e| sink.lock().unwrap().push(e));
    assert_eq!(agent.ask("hi").await.unwrap(), "role fallback");
    let events = log.lock().unwrap().clone();
    let [ChainEvent::Fallback { failed, chosen, .. }] = &events[..] else {
        panic!("expected one fallback, got {events:?}")
    };
    assert_eq!((failed.account(), chosen.account()), ("a", "b"));
}

#[tokio::test]
async fn a_per_agent_override_chain_falls_back_too() {
    let (broken, _m1) = replying(429, rate_limit_body()).await;
    let (good, _m2) = replying(200, completion("override fallback")).await;
    let (unused, m3) = replying(200, completion("role chain")).await;
    let m3 = m3.expect(0);
    let roster = Roster::new();
    roster.add_role(Role::new("writer", "Be brief").model_chain(vec![account(&unused, "r", "m")]));
    let agent = roster.hire("writer", "Ines").unwrap();
    agent.set_model_chain(vec![account(&broken, "x", "m"), account(&good, "y", "m")]);
    assert_eq!(agent.ask("hi").await.unwrap(), "override fallback");
    m3.assert_async().await;
}
