use archodin::graph::{
    ApprovalDecision, Approver, Checkpoint, CheckpointStore, Graph, InMemoryCheckpointStore,
    JobOutcome, ScriptedApprover,
};
use futures::StreamExt;

#[derive(Debug, Clone, PartialEq)]
struct Tally {
    total: u32,
}

fn add_ten_graph() -> Graph<Tally> {
    Graph::builder()
        .node("add_ten", |s: Tally| async move {
            Tally {
                total: s.total + 10,
            }
        })
        .entry("add_ten")
        .build()
        .unwrap()
}

fn gated_graph() -> Graph<Tally> {
    Graph::builder()
        .node_with_approval(
            "spend",
            |s: &Tally| format!("add 10 to {}?", s.total),
            |s: Tally| async move {
                Tally {
                    total: s.total + 10,
                }
            },
        )
        .entry("spend")
        .build()
        .unwrap()
}

#[tokio::test]
async fn each_step_writes_a_checkpoint() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let outcome = add_ten_graph()
        .run_job(
            "job-1",
            Tally { total: 1 },
            &store,
            &ScriptedApprover::new([]),
        )
        .await
        .unwrap();
    assert_eq!(outcome, JobOutcome::Finished(Tally { total: 11 }));
    let saved: Vec<Checkpoint<Tally>> = store.checkpoints("job-1").await;
    let states: Vec<_> = saved.iter().map(|c| (c.step, c.state.total)).collect();
    assert_eq!(states, vec![(0, 1), (1, 11)]);
    assert_eq!(saved[0].next.as_deref(), Some("add_ten"));
    assert_eq!(saved[1].next, None);
}

#[tokio::test]
async fn an_interrupted_job_resumes_and_matches_an_uninterrupted_run() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let graph = add_ten_graph();
    // Interrupt: the run is dropped after the Node started, before it finished.
    let idle = ScriptedApprover::new([]);
    let mut events = graph.stream_job("job-1", Tally { total: 1 }, &store, &idle);
    events.next().await; // JobStarted
    events.next().await; // NodeStarted
    drop(events);
    assert_eq!(store.checkpoints("job-1").await.len(), 1);

    // "Restart": a fresh Graph over the same store; the initial State is ignored.
    let resumed = add_ten_graph()
        .run_job(
            "job-1",
            Tally { total: 999 },
            &store,
            &ScriptedApprover::new([]),
        )
        .await
        .unwrap();
    let uninterrupted = add_ten_graph().run(Tally { total: 1 }).await.unwrap();
    assert_eq!(resumed, JobOutcome::Finished(uninterrupted));
}

#[tokio::test]
async fn a_step_requesting_approval_pauses_then_continues_on_approve() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let paused = gated_graph()
        .run_job(
            "job-1",
            Tally { total: 1 },
            &store,
            &ScriptedApprover::new([ApprovalDecision::Pending]),
        )
        .await
        .unwrap();
    assert_eq!(
        paused,
        JobOutcome::Paused {
            node: "spend".into(),
            prompt: "add 10 to 1?".into(),
            state: Tally { total: 1 }
        }
    );

    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let done = gated_graph()
        .run_job("job-1", Tally { total: 1 }, &store, &approver)
        .await
        .unwrap();
    assert_eq!(done, JobOutcome::Finished(Tally { total: 11 }));
    let asked = approver.requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].job, "job-1");
    assert_eq!(asked[0].node, "spend");
}

#[tokio::test]
async fn a_rejected_approval_stops_the_job() {
    let store = InMemoryCheckpointStore::<Tally>::new();
    let outcome = gated_graph()
        .run_job(
            "job-1",
            Tally { total: 1 },
            &store,
            &ScriptedApprover::new([ApprovalDecision::Rejected]),
        )
        .await
        .unwrap();
    assert_eq!(
        outcome,
        JobOutcome::Rejected {
            node: "spend".into(),
            state: Tally { total: 1 }
        }
    );
}

#[test]
fn the_approval_interface_is_a_trait_the_agora_can_implement() {
    fn takes_approver(_: &dyn Approver) {}
    takes_approver(&ScriptedApprover::new([]));
}
