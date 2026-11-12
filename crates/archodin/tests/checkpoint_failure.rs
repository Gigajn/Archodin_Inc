//! KR-04: when the Checkpoint store fails, a durable Job ends with a
//! terminal `JobEvent::CheckpointFailed` event and `Graph::run_job`
//! returns `Err(GraphError::CheckpointFailed(..))` — it never panics
//! (in particular, never in the `unreachable!` at the end of
//! `run_job_with`) and never ends the stream silently.

use archodin::graph::{
    Checkpoint, CheckpointStore, Graph, GraphError, InMemoryCheckpointStore, JobEvent,
    ScriptedApprover, StoreError,
};
use futures::future::BoxFuture;
use futures::StreamExt;

#[derive(Debug, Clone, PartialEq)]
struct Tally {
    total: u32,
}

fn add_one_graph() -> Graph<Tally> {
    Graph::builder()
        .node(
            "add_one",
            |s: Tally| async move { Tally { total: s.total + 1 } },
        )
        .entry("add_one")
        .build()
        .unwrap()
}

fn failing() -> StoreError {
    StoreError::Invalid("boom: the store is down".into())
}

/// A store whose every operation fails.
struct FailingStore;

impl CheckpointStore<Tally> for FailingStore {
    fn save(&self, _checkpoint: Checkpoint<Tally>) -> BoxFuture<'_, Result<(), StoreError>> {
        Box::pin(async { Err(failing()) })
    }

    fn checkpoints(&self, _job: &str) -> BoxFuture<'_, Result<Vec<Checkpoint<Tally>>, StoreError>> {
        Box::pin(async { Err(failing()) })
    }
}

/// A store that reads fine and saves once, then fails every later save:
/// the first save is the step-0 Checkpoint at the start of the Job, so a
/// failure here exercises the save after a Node runs.
struct FailAfterFirstSaveStore {
    inner: InMemoryCheckpointStore<Tally>,
    saves: std::sync::Mutex<usize>,
}

impl FailAfterFirstSaveStore {
    fn new() -> Self {
        Self {
            inner: InMemoryCheckpointStore::new(),
            saves: std::sync::Mutex::new(0),
        }
    }
}

impl CheckpointStore<Tally> for FailAfterFirstSaveStore {
    fn save(&self, checkpoint: Checkpoint<Tally>) -> BoxFuture<'_, Result<(), StoreError>> {
        let first = {
            let mut count = self.saves.lock().unwrap_or_else(|e| e.into_inner());
            *count += 1;
            *count == 1
        };
        if first {
            self.inner.save(checkpoint)
        } else {
            Box::pin(async { Err(failing()) })
        }
    }

    fn checkpoints(&self, job: &str) -> BoxFuture<'_, Result<Vec<Checkpoint<Tally>>, StoreError>> {
        self.inner.checkpoints(job)
    }
}

#[tokio::test]
async fn run_job_returns_checkpoint_failed_when_the_store_cannot_be_read() {
    let store = FailingStore;
    let err = add_one_graph()
        .run_job(
            "job-1",
            Tally { total: 0 },
            &store,
            &ScriptedApprover::new([]),
        )
        .await
        .expect_err("a Job whose Checkpoints cannot be read must return Err, not panic");
    assert!(matches!(
        err,
        GraphError::CheckpointFailed(StoreError::Invalid(_))
    ));
}

#[tokio::test]
async fn stream_ends_with_a_terminal_checkpoint_failed_event() {
    let store = FailingStore;
    let events: Vec<_> = add_one_graph()
        .stream_job(
            "job-1",
            Tally { total: 0 },
            &store,
            &ScriptedApprover::new([]),
        )
        .collect()
        .await;
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        JobEvent::CheckpointFailed {
            error: StoreError::Invalid(_)
        }
    ));
}

#[tokio::test]
async fn a_failed_save_after_a_node_started_ends_the_stream_without_node_finished() {
    // The save after the Node runs fails, so `NodeFinished` is suppressed:
    // consumers see `NodeStarted` without its pair, then the terminal event.
    let store = FailAfterFirstSaveStore::new();
    let events: Vec<_> = add_one_graph()
        .stream_job(
            "job-1",
            Tally { total: 0 },
            &store,
            &ScriptedApprover::new([]),
        )
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], JobEvent::JobStarted));
    assert!(matches!(
        &events[1],
        JobEvent::NodeStarted { node } if node.as_str() == "add_one"
    ));
    assert!(matches!(
        &events[2],
        JobEvent::CheckpointFailed {
            error: StoreError::Invalid(_)
        }
    ));

    // The same store, resumed: the step-0 Checkpoint is there, the Node
    // runs again, and its save fails the same way — `run_job` is `Err`.
    let err = add_one_graph()
        .run_job(
            "job-1",
            Tally { total: 0 },
            &store,
            &ScriptedApprover::new([]),
        )
        .await
        .expect_err("a Job whose Checkpoint save fails must return Err, not panic");
    assert!(matches!(
        err,
        GraphError::CheckpointFailed(StoreError::Invalid(_))
    ));
}
