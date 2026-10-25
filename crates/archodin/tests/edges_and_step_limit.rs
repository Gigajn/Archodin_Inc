use archodin::graph::{Graph, GraphError, JobEvent, RunOptions, END};
use futures::StreamExt;

#[derive(Debug, Clone, PartialEq)]
struct Ticket {
    urgent: bool,
    trail: Vec<&'static str>,
}

fn visit(name: &'static str) -> impl Fn(Ticket) -> std::future::Ready<Ticket> + Send + Sync {
    move |mut t| {
        t.trail.push(name);
        std::future::ready(t)
    }
}

fn triage_graph() -> Graph<Ticket> {
    Graph::builder()
        .node("triage", visit("triage"))
        .node("escalate", visit("escalate"))
        .node("archive", visit("archive"))
        .entry("triage")
        .conditional_edge(
            "triage",
            |t: &Ticket| if t.urgent { "escalate" } else { "archive" },
            &["escalate", "archive"],
        )
        .edge("escalate", END)
        .build()
        .expect("valid graph")
}

fn ticket(urgent: bool) -> Ticket {
    Ticket {
        urgent,
        trail: vec![],
    }
}

#[tokio::test]
async fn a_conditional_edge_routes_by_state() {
    let urgent = triage_graph().run(ticket(true)).await.unwrap();
    assert_eq!(urgent.trail, vec!["triage", "escalate"]);
    let calm = triage_graph().run(ticket(false)).await.unwrap();
    assert_eq!(calm.trail, vec!["triage", "archive"]);
}

#[tokio::test]
async fn plain_edges_chain_nodes_in_order() {
    let graph = Graph::builder()
        .node("a", visit("a"))
        .node("b", visit("b"))
        .node("c", visit("c"))
        .entry("a")
        .edge("a", "b")
        .edge("b", "c")
        .build()
        .unwrap();
    let out = graph.run(ticket(false)).await.unwrap();
    assert_eq!(out.trail, vec!["a", "b", "c"]);
}

fn looping_graph() -> Graph<Ticket> {
    Graph::builder()
        .node("spin", visit("spin"))
        .entry("spin")
        .edge("spin", "spin")
        .build()
        .unwrap()
}

#[tokio::test]
async fn a_looping_graph_stops_at_the_default_step_limit_of_25() {
    let mut events = looping_graph().stream(ticket(false));
    let mut started = 0;
    let mut last = None;
    while let Some(e) = events.next().await {
        if matches!(e, JobEvent::NodeStarted { .. }) {
            started += 1;
        }
        last = Some(e);
    }
    assert_eq!(started, 25);
    match last.unwrap() {
        JobEvent::StepLimitReached { limit, state } => {
            assert_eq!(limit, 25);
            assert_eq!(state.trail.len(), 25);
        }
        other => panic!("expected StepLimitReached, got {other:?}"),
    }
    assert_eq!(
        looping_graph().run(ticket(false)).await,
        Err(GraphError::StepLimitReached { limit: 25 })
    );
}

#[tokio::test]
async fn the_step_limit_can_be_set_per_run() {
    let graph = looping_graph();
    let err = graph
        .run_with(ticket(false), RunOptions::new().step_limit(3))
        .await;
    assert_eq!(err, Err(GraphError::StepLimitReached { limit: 3 }));
    let events: Vec<_> = graph
        .stream_with(ticket(false), RunOptions::new().step_limit(2))
        .collect()
        .await;
    assert!(matches!(
        events.last(),
        Some(JobEvent::StepLimitReached { limit: 2, .. })
    ));
}

#[tokio::test]
async fn a_run_that_ends_exactly_at_the_limit_finishes_normally() {
    let out = triage_graph()
        .run_with(ticket(true), RunOptions::new().step_limit(2))
        .await
        .unwrap();
    assert_eq!(out.trail, vec!["triage", "escalate"]);
}

#[test]
fn edges_to_unknown_nodes_are_rejected() {
    let r = Graph::<Ticket>::builder()
        .node("a", visit("a"))
        .entry("a")
        .edge("a", "nowhere")
        .build();
    assert!(matches!(r, Err(GraphError::UnknownEdgeNode(n)) if n == "nowhere"));
}
