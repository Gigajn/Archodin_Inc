//! KR-06 + KR-11: a Job must fail with an error, never panic, when its graph
//! is wrong; an Agent's conversation must stop growing; a fixed API key must
//! never leak into error text (the redaction itself is unit-tested in
//! `agent.rs`, where `Provider::redact` lives).
//!
//! No test here reaches the network: the only HTTP is mockito on localhost,
//! the same seam the existing agent tests use.

use archodin::graph::{
    Graph, GraphError, InMemoryCheckpointStore, JobEvent, JobOutcome, Provider, Role, Roster,
    ScriptedApprover, END,
};
use futures::StreamExt;
use mockito::Server;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
struct Ticket {
    urgent: bool,
    trail: Vec<&'static str>,
}

fn ticket(urgent: bool) -> Ticket {
    Ticket {
        urgent,
        trail: vec![],
    }
}

fn visit(name: &'static str) -> impl Fn(Ticket) -> std::future::Ready<Ticket> + Send + Sync {
    move |mut t| {
        t.trail.push(name);
        std::future::ready(t)
    }
}


/// A graph whose conditional router returns a target that is not declared.
fn undeclared_target_graph() -> Graph<Ticket> {
    Graph::builder()
        .node("triage", visit("triage"))
        .node("escalate", visit("escalate"))
        .node("archive", visit("archive"))
        .entry("triage")
        .conditional_edge("triage", |_| "nowhere", &["escalate", "archive"])
        .edge("escalate", END)
        .edge("archive", END)
        .build()
        .unwrap()
}

#[tokio::test]
async fn a_conditional_edge_returning_an_undeclared_target_fails_instead_of_panicking() {
    let err = undeclared_target_graph().run(ticket(false)).await;
    assert_eq!(
        err,
        Err(GraphError::UnknownConditionalTarget {
            node: "triage".into(),
            target: "nowhere".into(),
        })
    );
}

#[tokio::test]
async fn a_conditional_edge_returning_an_undeclared_target_fails_a_durable_job_too() {
    let store = InMemoryCheckpointStore::<Ticket>::new();
    let err = undeclared_target_graph()
        .run_job(
            "job-1",
            ticket(false),
            &store,
            &ScriptedApprover::new([]),
        )
        .await;
    assert_eq!(
        err,
        Err(GraphError::UnknownConditionalTarget {
            node: "triage".into(),
            target: "nowhere".into(),
        })
    );
}
#[derive(Debug, Clone, PartialEq)]
struct Tally {
    total: u32,
}

fn tally_graph() -> Graph<Tally> {
    Graph::builder()
        .node("gone", |s: Tally| async move {
            Tally { total: s.total + 1 }
        })
        .node("after", |s: Tally| async move {
            Tally { total: s.total + 10 }
        })
        .edge("gone", "after")
        .entry("gone")
        .build()
        .unwrap()
}

/// The graph a later version of the program has: the node the checkpoint
/// points at no longer exists.
fn regraphed() -> Graph<Tally> {
    Graph::builder()
        .node("other", |s: Tally| async move {
            Tally { total: s.total + 100 }
        })
        .entry("other")
        .build()
        .unwrap()
}

#[tokio::test]
async fn a_resumed_job_naming_a_node_the_graph_no_longer_has_fails_instead_of_panicking() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let idle = ScriptedApprover::new([]);
    // Interrupt the first run after its Node started: the checkpoint says the
    // next node is "gone".
    let mut events = tally_graph().stream_job("job-1", Tally { total: 0 }, &store, &idle);
    events.next().await; // JobStarted
    events.next().await; // NodeStarted
    drop(events);
    assert_eq!(store.checkpoints("job-1").await.len(), 1);

    // Resuming against the new graph must report the missing node as an error.
    let mut events = regraphed().stream_job("job-1", Tally { total: 999 }, &store, &idle);
    let mut last = None;
    while let Some(e) = events.next().await {
        last = Some(e);
    }
    assert!(
        matches!(
            last,
            Some(JobEvent::JobFailed {
                error: GraphError::UnknownEdgeNode(ref n)
            }) if n == "gone"
        ),
        "expected a JobFailed for node `gone`, got {last:?}"
    );

    let outcome = regraphed()
        .run_job("job-1", Tally { total: 999 }, &store, &idle)
        .await;
    assert_eq!(outcome, Err(GraphError::UnknownEdgeNode("gone".into())));
}

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

#[tokio::test]
async fn an_agent_conversation_stops_growing_at_100_messages() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .with_header("content-type", "application/json")
        .with_body(completion("ok"))
        .create_async()
        .await;
    let roster = Roster::new();
    roster.add_role(
        Role::new("Writer", "Be brief")
            .model_chain(vec![Provider::openai_compatible(server.url(), "key", "m")]),
    );
    let nova = roster.hire("Writer", "Nova").unwrap();

    // 60 asks add 2 turns each: without a cap the history would hold 120.
    for _ in 0..60 {
        assert_eq!(nova.ask("hello").await.unwrap(), "ok");
    }
    assert_eq!(nova.history_len(), 100);

    // The agent still answers after the cap, and the history stays capped.
    assert_eq!(nova.ask("still there?").await.unwrap(), "ok");
    assert_eq!(nova.history_len(), 100);
}

// The stream counterpart of the resume test above: a durable Job that ends
// normally still reports its outcome, so the new JobFailed event does not
// swallow the happy path.
#[tokio::test]
async fn a_durable_job_that_finishes_still_reports_finished() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let outcome = tally_graph()
        .run_job("job-2", Tally { total: 0 }, &store, &ScriptedApprover::new([]))
        .await
        .unwrap();
    assert_eq!(outcome, JobOutcome::Finished(Tally { total: 11 }));
}
