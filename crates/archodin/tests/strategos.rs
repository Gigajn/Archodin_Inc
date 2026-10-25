use std::sync::{Arc, Mutex};

use archodin::graph::{
    AgentNode, Assignment, Graph, JobEvent, Provider, Strategos, StrategosError, StrategosEvent,
    END,
};
use futures::StreamExt;
use mockito::{Matcher, Server, ServerGuard};
use serde_json::json;

#[derive(Debug, Clone, Default, PartialEq)]
struct Job {
    request: String,
    handled: Vec<Assignment>,
    failure: Option<StrategosError>,
    inner_note: String,
}

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

/// A fake Provider whose answer depends on the system instructions it receives.
async fn fake(replies: &[(&str, &str)]) -> ServerGuard {
    let mut server = Server::new_async().await;
    for (instructions, reply) in replies {
        server
            .mock("POST", "/chat/completions")
            .match_body(Matcher::Regex(format!("You are {instructions}")))
            .with_header("content-type", "application/json")
            .with_body(completion(reply))
            .create_async()
            .await;
    }
    server
}

fn agent(server: &ServerGuard, who: &str) -> AgentNode {
    let provider = Provider::openai_compatible(server.url(), "key", "any-model");
    AgentNode::new(provider, format!("You are {who}."))
}

type Sink = Arc<Mutex<Vec<StrategosEvent>>>;

fn observed(s: Strategos, sink: &Sink) -> Strategos {
    let sink = sink.clone();
    s.observe(move |e| sink.lock().unwrap().push(e))
}

fn job_graph(strategos: Strategos) -> Graph<Job> {
    Graph::builder()
        .strategos_node(
            "lead",
            strategos,
            |s: &Job| s.request.clone(),
            |mut s, r| {
                match r {
                    Ok(h) => s.handled = h,
                    Err(e) => s.failure = Some(e),
                }
                s
            },
        )
        .entry("lead")
        .edge("lead", END)
        .build()
        .unwrap()
}

fn job(request: &str) -> Job {
    Job {
        request: request.into(),
        ..Job::default()
    }
}

fn assigned(sink: &Sink) -> Vec<String> {
    sink.lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            StrategosEvent::PieceAssigned {
                strategos, agent, ..
            } => Some(format!("{strategos}->{agent}")),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn strategos_splits_a_job_and_assigns_pieces_to_named_agents() {
    let server = fake(&[
        (
            "the planner",
            r#"[{"agent":"writer","task":"draft"},{"agent":"checker","task":"review"}]"#,
        ),
        ("the writer", "a draft"),
        ("the checker", "looks good"),
    ])
    .await;
    let sink: Sink = Default::default();
    let strategos = observed(
        Strategos::new("Lead", agent(&server, "the planner"))
            .agent("writer", agent(&server, "the writer"))
            .agent("checker", agent(&server, "the checker")),
        &sink,
    );

    let done = job_graph(strategos).run(job("write a post")).await.unwrap();

    let got: Vec<_> = done
        .handled
        .iter()
        .map(|a| {
            (
                a.piece.agent.as_str(),
                a.piece.task.as_str(),
                a.result.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("writer", "draft", "a draft"),
            ("checker", "review", "looks good")
        ]
    );
    assert_eq!(assigned(&sink), ["Lead->writer", "Lead->checker"]);
}

#[tokio::test]
async fn unknown_member_in_the_plan_is_reported_in_the_state() {
    let server = fake(&[("the planner", r#"[{"agent":"ghost","task":"x"}]"#)]).await;
    let strategos = Strategos::new("Lead", agent(&server, "the planner"));
    let done = job_graph(strategos).run(job("x")).await.unwrap();
    assert_eq!(
        done.failure,
        Some(StrategosError::UnknownMember("ghost".into()))
    );
}

#[tokio::test]
async fn a_whole_graph_runs_as_a_node_and_shares_results_through_state() {
    let inner: Graph<String> = Graph::builder()
        .node("shout", |s: String| async move { s.to_uppercase() })
        .entry("shout")
        .edge("shout", END)
        .build()
        .unwrap();
    let outer: Graph<Job> = Graph::builder()
        .graph_node(
            "sub",
            inner,
            |s: &Job| s.request.clone(),
            |mut s, r| {
                s.inner_note = r.unwrap();
                s
            },
        )
        .entry("sub")
        .edge("sub", END)
        .build()
        .unwrap();
    assert_eq!(outer.run(job("hello")).await.unwrap().inner_note, "HELLO");
}

#[tokio::test]
async fn two_level_department_completes_a_job_and_events_show_who_handled_what() {
    let server = fake(&[
        (
            "the chief",
            r#"Plan: [{"agent":"marketing","task":"launch post"},{"agent":"lawyer","task":"check terms"}]"#,
        ),
        (
            "the marketing lead",
            r#"[{"agent":"writer","task":"write copy"}]"#,
        ),
        ("the writer", "fresh copy"),
        ("the lawyer", "terms ok"),
    ])
    .await;
    let sink: Sink = Default::default();
    let marketing = job_graph(observed(
        Strategos::new("Marketing", agent(&server, "the marketing lead"))
            .agent("writer", agent(&server, "the writer")),
        &sink,
    ));
    let top = observed(
        Strategos::new("Chief", agent(&server, "the chief"))
            .department("marketing", marketing, job, |s| {
                s.handled
                    .iter()
                    .map(|a| a.result.clone())
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .agent("lawyer", agent(&server, "the lawyer")),
        &sink,
    );

    let mut events = job_graph(top).stream(job("launch"));
    let mut last = None;
    while let Some(e) = events.next().await {
        if let JobEvent::JobFinished { state } = e {
            last = Some(state);
        }
    }
    let done = last.unwrap();
    let results: Vec<_> = done.handled.iter().map(|a| a.result.as_str()).collect();
    assert_eq!(results, ["fresh copy", "terms ok"]);
    assert_eq!(
        assigned(&sink),
        ["Chief->marketing", "Marketing->writer", "Chief->lawyer"]
    );
}
