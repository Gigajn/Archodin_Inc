use std::sync::{Arc, Mutex};

use archodin::graph::{Fanout, JobEvent, MergeRules};
use futures::channel::oneshot;
use futures::StreamExt;

#[derive(Debug, Clone, PartialEq, Default)]
struct Desk {
    messages: Vec<String>,
    headline: String,
    verdict: Option<String>,
}

fn rules() -> MergeRules<Desk> {
    MergeRules::new()
        .append("messages", |s: &mut Desk| &mut s.messages)
        .decision("verdict", |s: &mut Desk| &mut s.verdict)
}

type Slot<T> = Arc<Mutex<Option<T>>>;

fn slot<T>(v: T) -> Slot<T> {
    Arc::new(Mutex::new(Some(v)))
}

#[tokio::test]
async fn two_nodes_fanned_out_from_one_node_run_concurrently_and_both_reach_the_state() {
    // Each branch waits for a signal from the other, so they only finish if they run at once.
    let (tx_a, rx_a) = oneshot::channel::<()>();
    let (tx_b, rx_b) = oneshot::channel::<()>();
    let (tx_a, rx_a, tx_b, rx_b) = (slot(tx_a), slot(rx_a), slot(tx_b), slot(rx_b));

    let fanout = Fanout::builder(rules())
        .from("plan", |mut s: Desk| async move {
            s.messages.push("planned".into());
            s
        })
        .node("a", move |mut s: Desk| {
            let (tx, rx) = (tx_a.clone(), rx_b.clone());
            async move {
                let tx = tx.lock().unwrap().take().unwrap();
                let rx = rx.lock().unwrap().take().unwrap();
                tx.send(()).unwrap();
                rx.await.unwrap();
                s.messages.push("from a".into());
                s
            }
        })
        .node("b", move |mut s: Desk| {
            let (tx, rx) = (tx_b.clone(), rx_a.clone());
            async move {
                let tx = tx.lock().unwrap().take().unwrap();
                let rx = rx.lock().unwrap().take().unwrap();
                tx.send(()).unwrap();
                rx.await.unwrap();
                s.messages.push("from b".into());
                s
            }
        })
        .build()
        .unwrap();

    let state = fanout.run(Desk::default()).await.unwrap();
    assert_eq!(state.messages, vec!["planned", "from a", "from b"]);
}

#[tokio::test]
async fn an_append_field_keeps_every_parallel_nodes_entries() {
    let fanout = Fanout::builder(rules())
        .node("a", |mut s: Desk| async move {
            s.messages.push("a1".into());
            s.messages.push("a2".into());
            s
        })
        .node("b", |mut s: Desk| async move {
            s.messages.push("b1".into());
            s
        })
        .build()
        .unwrap();
    let initial = Desk {
        messages: vec!["start".into()],
        ..Desk::default()
    };
    let state = fanout.run(initial).await.unwrap();
    assert_eq!(state.messages, vec!["start", "a1", "a2", "b1"]);
}

#[tokio::test]
async fn an_overwrite_field_takes_a_single_value_deterministically() {
    for _ in 0..10 {
        let fanout = Fanout::builder(rules())
            .node("a", |mut s: Desk| async move {
                tokio::task::yield_now().await;
                s.headline = "from a".into();
                s
            })
            .node("b", |mut s: Desk| async move {
                s.headline = "from b".into();
                s
            })
            .build()
            .unwrap();
        // The later-declared Node wins, whatever order they finish in.
        let state = fanout.run(Desk::default()).await.unwrap();
        assert_eq!(state.headline, "from b");
    }
}

#[tokio::test]
async fn competing_writes_to_a_decision_field_surface_as_a_conflict_event() {
    let fanout = Fanout::builder(rules())
        .node("a", |mut s: Desk| async move {
            s.verdict = Some("ship".into());
            s
        })
        .node("b", |mut s: Desk| async move {
            s.verdict = Some("hold".into());
            s
        })
        .build()
        .unwrap();
    let events: Vec<_> = fanout.stream(Desk::default()).collect().await;
    assert!(events.contains(&JobEvent::Conflict {
        field: "verdict".into(),
        nodes: vec!["a".into(), "b".into()],
    }));
    // Left for the Qadi: the field keeps its previous value for now.
    match events.last().unwrap() {
        JobEvent::JobFinished { state } => assert_eq!(state.verdict, None),
        other => panic!("unexpected last event {other:?}"),
    }
}

#[tokio::test]
async fn a_single_writer_to_a_decision_field_is_no_conflict() {
    let fanout = Fanout::builder(rules())
        .node("a", |mut s: Desk| async move {
            s.verdict = Some("ship".into());
            s
        })
        .node("b", |s: Desk| async move { s })
        .build()
        .unwrap();
    let events: Vec<_> = fanout.stream(Desk::default()).collect().await;
    assert!(!events
        .iter()
        .any(|e| matches!(e, JobEvent::Conflict { .. })));
    match events.last().unwrap() {
        JobEvent::JobFinished { state } => assert_eq!(state.verdict.as_deref(), Some("ship")),
        other => panic!("unexpected last event {other:?}"),
    }
}

#[tokio::test]
async fn parallel_nodes_stream_started_then_finished_events_in_declaration_order() {
    let fanout = Fanout::builder(rules())
        .node("a", |s: Desk| async move { s })
        .node("b", |s: Desk| async move { s })
        .build()
        .unwrap();
    let events: Vec<_> = fanout.stream(Desk::default()).collect().await;
    let names: Vec<String> = events
        .iter()
        .map(|e| match e {
            JobEvent::JobStarted => "start".into(),
            JobEvent::NodeStarted { node } => format!("+{node}"),
            JobEvent::NodeFinished { node } => format!("-{node}"),
            JobEvent::JobFinished { .. } => "end".into(),
            JobEvent::Conflict { .. } => "conflict".into(),
            _ => "other".into(),
        })
        .collect();
    assert_eq!(names, ["start", "+a", "+b", "-a", "-b", "end"]);
}
