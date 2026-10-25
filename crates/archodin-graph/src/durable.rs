//! Checkpoints, resume and pausing for approval.
//!
//! A durable Job writes a [`Checkpoint`] of the State before its first step and
//! after every step. Running the same Job id again resumes from the latest
//! Checkpoint. A Node added with [`GraphBuilder::node_with_approval`] asks an
//! [`Approver`] (the interface the Agora will implement) before it runs: the Job
//! pauses while the answer is [`ApprovalDecision::Pending`], continues on
//! approve and stops on reject.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};

use crate::{Edge, Graph, GraphBuilder, GraphError, JobEvent, RunOptions, END};

/// A saved copy of the State after a step.
#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint<S> {
    /// The Job this Checkpoint belongs to.
    pub job: String,
    /// How many steps had completed when it was saved (0 before the first step).
    pub step: usize,
    /// The State at that point.
    pub state: S,
    /// The Node to run next, or `None` when the Job has finished.
    pub next: Option<String>,
}

/// Where Checkpoints are kept.
pub trait CheckpointStore<S>: Send + Sync {
    /// Appends a Checkpoint for its Job.
    fn save(&self, checkpoint: Checkpoint<S>) -> BoxFuture<'_, ()>;
    /// All Checkpoints of a Job, oldest first.
    fn checkpoints(&self, job: &str) -> BoxFuture<'_, Vec<Checkpoint<S>>>;
}

/// A [`CheckpointStore`] held in memory.
pub struct InMemoryCheckpointStore<S> {
    saved: Mutex<HashMap<String, Vec<Checkpoint<S>>>>,
}

impl<S> InMemoryCheckpointStore<S> {
    /// An empty store.
    pub fn new() -> Self {
        Self {
            saved: Mutex::new(HashMap::new()),
        }
    }
}

impl<S> Default for InMemoryCheckpointStore<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Clone + Send + 'static> CheckpointStore<S> for InMemoryCheckpointStore<S> {
    fn save(&self, checkpoint: Checkpoint<S>) -> BoxFuture<'_, ()> {
        self.saved
            .lock()
            .unwrap()
            .entry(checkpoint.job.clone())
            .or_default()
            .push(checkpoint);
        Box::pin(async {})
    }

    fn checkpoints(&self, job: &str) -> BoxFuture<'_, Vec<Checkpoint<S>>> {
        let found = self
            .saved
            .lock()
            .unwrap()
            .get(job)
            .cloned()
            .unwrap_or_default();
        Box::pin(async move { found })
    }
}

/// What a step asks a person to approve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// The Job asking.
    pub job: String,
    /// The Node asking.
    pub node: String,
    /// What the person is asked to approve.
    pub prompt: String,
}

/// A person's answer to an [`ApprovalRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Continue the Job.
    Approved,
    /// Stop the Job.
    Rejected,
    /// No answer yet: the Job pauses and can be resumed later.
    Pending,
}

/// The approval interface; the Agora will implement it.
pub trait Approver: Send + Sync {
    /// Answers a request.
    fn decide(&self, request: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision>;
}

/// A test double for [`Approver`]: answers from a script, in order, and records
/// every request. When the script runs out it answers `Pending`.
pub struct ScriptedApprover {
    script: Mutex<VecDeque<ApprovalDecision>>,
    asked: Mutex<Vec<ApprovalRequest>>,
}

impl ScriptedApprover {
    /// Builds an approver that gives these decisions in order.
    pub fn new(decisions: impl IntoIterator<Item = ApprovalDecision>) -> Self {
        Self {
            script: Mutex::new(decisions.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Every request received so far.
    pub fn requests(&self) -> Vec<ApprovalRequest> {
        self.asked.lock().unwrap().clone()
    }
}

impl Approver for ScriptedApprover {
    fn decide(&self, request: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision> {
        self.asked.lock().unwrap().push(request.clone());
        let decision = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(ApprovalDecision::Pending);
        Box::pin(async move { decision })
    }
}

/// How a durable Job ended or stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum JobOutcome<S> {
    /// The Job ran to the end.
    Finished(S),
    /// A step is waiting for a person; run the Job again to continue.
    Paused {
        node: String,
        prompt: String,
        state: S,
    },
    /// A person rejected a step; the Job stopped.
    Rejected { node: String, state: S },
}

pub(crate) type PromptFn<S> = Arc<dyn Fn(&S) -> String + Send + Sync>;

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a plain-function Node that asks for approval before it runs.
    pub fn node_with_approval<P, F, Fut>(mut self, name: &str, prompt: P, f: F) -> Self
    where
        P: Fn(&S) -> String + Send + Sync + 'static,
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = S> + Send + 'static,
    {
        self.approvals.insert(name.to_string(), Arc::new(prompt));
        self.node(name, f)
    }
}

enum Phase<S> {
    Begin(S),
    At { state: S, node: String, step: usize },
    Run { state: S, node: String, step: usize },
    Finish(S),
    Done,
}

impl<S: Clone + Send + 'static> Graph<S> {
    /// The Node after `_node` given the State; `None` ends the Job.
    fn next_after(&self, node: &str, state: &S) -> Option<String> {
        let next = match self.inner.edges.get(node) {
            None => END,
            Some(Edge::To(t)) => t.as_str(),
            Some(Edge::Conditional { router, targets }) => {
                let chosen = router(state);
                assert!(
                    targets.iter().any(|t| t == chosen),
                    "conditional edge from `{node}` returned undeclared target `{chosen}`"
                );
                chosen
            }
        };
        (next != END).then(|| next.to_string())
    }

    /// Streams a durable Job: Checkpoints are written as it goes, and an
    /// existing Checkpoint for `job` is resumed instead of starting over.
    pub fn stream_job<'a>(
        &'a self,
        job: &str,
        initial: S,
        store: &'a dyn CheckpointStore<S>,
        approver: &'a dyn Approver,
    ) -> BoxStream<'a, JobEvent<S>> {
        self.stream_job_with(job, initial, store, approver, RunOptions::default())
    }

    /// Like [`Graph::stream_job`] with `options`. The Step limit counts the steps
    /// already completed in earlier runs of the same Job, so it holds across resume.
    pub fn stream_job_with<'a>(
        &'a self,
        job: &str,
        initial: S,
        store: &'a dyn CheckpointStore<S>,
        approver: &'a dyn Approver,
        options: RunOptions,
    ) -> BoxStream<'a, JobEvent<S>> {
        let limit = options.step_limit;
        let job = job.to_string();
        stream::unfold(Phase::Begin(initial), move |phase| {
            let job = job.clone();
            async move {
                match phase {
                    Phase::Begin(initial) => {
                        let latest = store.checkpoints(&job).await.pop();
                        let next = match latest {
                            None => {
                                store
                                    .save(Checkpoint {
                                        job: job.clone(),
                                        step: 0,
                                        state: initial.clone(),
                                        next: Some(self.inner.entry.clone()),
                                    })
                                    .await;
                                Phase::At {
                                    state: initial,
                                    node: self.inner.entry.clone(),
                                    step: 0,
                                }
                            }
                            Some(cp) => match cp.next {
                                Some(node) => Phase::At {
                                    state: cp.state,
                                    node,
                                    step: cp.step,
                                },
                                None => Phase::Finish(cp.state),
                            },
                        };
                        Some((JobEvent::JobStarted, next))
                    }
                    Phase::At { state, step, .. } if step >= limit => {
                        Some((JobEvent::StepLimitReached { limit, state }, Phase::Done))
                    }
                    Phase::At { state, node, step } => {
                        if let Some(prompt) = self.inner.approvals.get(&node) {
                            let prompt = prompt(&state);
                            let request = ApprovalRequest {
                                job: job.clone(),
                                node: node.clone(),
                                prompt: prompt.clone(),
                            };
                            match approver.decide(&request).await {
                                ApprovalDecision::Approved => {}
                                ApprovalDecision::Rejected => {
                                    return Some((
                                        JobEvent::JobRejected { node, state },
                                        Phase::Done,
                                    ));
                                }
                                ApprovalDecision::Pending => {
                                    return Some((
                                        JobEvent::JobPaused {
                                            node,
                                            prompt,
                                            state,
                                        },
                                        Phase::Done,
                                    ));
                                }
                            }
                        }
                        Some((
                            JobEvent::NodeStarted { node: node.clone() },
                            Phase::Run { state, node, step },
                        ))
                    }
                    Phase::Run { state, node, step } => {
                        let updated = (self.inner.nodes[&node])(state).await;
                        let next = self.next_after(&node, &updated);
                        store
                            .save(Checkpoint {
                                job: job.clone(),
                                step: step + 1,
                                state: updated.clone(),
                                next: next.clone(),
                            })
                            .await;
                        let phase = match next {
                            Some(n) => Phase::At {
                                state: updated,
                                node: n,
                                step: step + 1,
                            },
                            None => Phase::Finish(updated),
                        };
                        Some((JobEvent::NodeFinished { node }, phase))
                    }
                    Phase::Finish(state) => Some((JobEvent::JobFinished { state }, Phase::Done)),
                    Phase::Done => None,
                }
            }
        })
        .boxed()
    }

    /// Runs a durable Job until it finishes, pauses for approval or is rejected.
    pub async fn run_job(
        &self,
        job: &str,
        initial: S,
        store: &dyn CheckpointStore<S>,
        approver: &dyn Approver,
    ) -> Result<JobOutcome<S>, GraphError> {
        self.run_job_with(job, initial, store, approver, RunOptions::default())
            .await
    }

    /// Like [`Graph::run_job`] with `options`; hitting the Step limit returns
    /// [`GraphError::StepLimitReached`], and the Job can resume with a higher limit.
    pub async fn run_job_with(
        &self,
        job: &str,
        initial: S,
        store: &dyn CheckpointStore<S>,
        approver: &dyn Approver,
        options: RunOptions,
    ) -> Result<JobOutcome<S>, GraphError> {
        let mut events = self.stream_job_with(job, initial, store, approver, options);
        while let Some(event) = events.next().await {
            match event {
                JobEvent::JobFinished { state } => return Ok(JobOutcome::Finished(state)),
                JobEvent::JobPaused {
                    node,
                    prompt,
                    state,
                } => {
                    return Ok(JobOutcome::Paused {
                        node,
                        prompt,
                        state,
                    })
                }
                JobEvent::StepLimitReached { limit, .. } => {
                    return Err(GraphError::StepLimitReached { limit })
                }
                JobEvent::JobRejected { node, state } => {
                    return Ok(JobOutcome::Rejected { node, state })
                }
                _ => {}
            }
        }
        unreachable!("a durable Job stream always ends with a closing event")
    }
}
