//! Parallel Nodes and per-field merge rules (ADR 0002).
//!
//! A [`Fanout`] runs several Nodes at the same time, each starting from the same State.
//! Their updates are combined by [`MergeRules`], chosen per State field:
//!
//! - overwrite (the default for every field without a rule): the Node declared last wins,
//!   whatever order the Nodes finish in;
//! - append ([`MergeRules::append`]): for list fields such as messages, every Node's new
//!   entries are kept, in declaration order;
//! - decision ([`MergeRules::decision`]): competing values are left for the Qadi and
//!   reported as a [`JobEvent::Conflict`]; the field keeps its previous value meanwhile.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use futures::stream::{self, BoxStream, StreamExt};

use crate::JobEvent;

type NodeFn<S> = Arc<dyn Fn(S) -> BoxFuture<'static, S> + Send + Sync>;

/// A merge rule for one field: reads the base State and each branch's State (with the
/// branch Node names), writes into the merged State and reports conflicts.
type Rule<S> = Arc<dyn Fn(&S, &[S], &[String], &mut S, &mut Vec<Conflict>) + Send + Sync>;

struct Conflict {
    field: String,
    nodes: Vec<String>,
}

/// How the parallel Nodes' updates are combined, field by field.
pub struct MergeRules<S> {
    rules: Vec<Rule<S>>,
}

impl<S> Default for MergeRules<S> {
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}

impl<S: Clone + Send + Sync + 'static> MergeRules<S> {
    /// Rules where every field is overwritten (the last declared Node wins).
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes a list field append: every Node's new entries are kept.
    pub fn append<T, A>(mut self, _field: &str, access: A) -> Self
    where
        T: Clone + Send + Sync + 'static,
        A: Fn(&mut S) -> &mut Vec<T> + Send + Sync + 'static,
    {
        self.rules
            .push(Arc::new(move |base, branches, _names, out, _c| {
                let mut base = base.clone();
                let base_len = access(&mut base).len();
                let mut merged: Vec<T> = access(&mut base).clone();
                for branch in branches {
                    let mut branch = branch.clone();
                    merged.extend(access(&mut branch).iter().skip(base_len).cloned());
                }
                *access(out) = merged;
            }));
        self
    }

    /// Marks a field as needing a decision: if Nodes write different values to it, the
    /// field keeps its previous value and a [`JobEvent::Conflict`] is reported.
    pub fn decision<T, A>(mut self, field: &str, access: A) -> Self
    where
        T: Clone + PartialEq + Send + Sync + 'static,
        A: Fn(&mut S) -> &mut T + Send + Sync + 'static,
    {
        let field = field.to_string();
        self.rules
            .push(Arc::new(move |base, branches, names, out, conflicts| {
                let mut base = base.clone();
                let before = access(&mut base).clone();
                let writers: Vec<(&String, T)> = branches
                    .iter()
                    .zip(names)
                    .filter_map(|(b, name)| {
                        let mut b = b.clone();
                        let v = access(&mut b).clone();
                        (v != before).then_some((name, v))
                    })
                    .collect();
                match writers.split_first() {
                    None => *access(out) = before,
                    Some(((_, first), rest)) if rest.iter().all(|(_, v)| v == first) => {
                        *access(out) = first.clone();
                    }
                    Some(_) => {
                        *access(out) = before;
                        conflicts.push(Conflict {
                            field: field.clone(),
                            nodes: writers.iter().map(|(n, _)| (*n).clone()).collect(),
                        });
                    }
                }
            }));
        self
    }

    fn merge(&self, base: &S, branches: Vec<S>, names: &[String]) -> (S, Vec<Conflict>) {
        let mut merged = branches.last().cloned().unwrap_or_else(|| base.clone());
        let mut conflicts = Vec::new();
        for rule in &self.rules {
            rule(base, &branches, names, &mut merged, &mut conflicts);
        }
        (merged, conflicts)
    }
}

/// Why a [`Fanout`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FanoutError {
    /// No parallel Node was added.
    #[error("the fanout has no parallel nodes")]
    NoNodes,
}

/// Builds a [`Fanout`].
pub struct FanoutBuilder<S> {
    rules: MergeRules<S>,
    source: Option<(String, NodeFn<S>)>,
    nodes: Vec<(String, NodeFn<S>)>,
}

impl<S: Clone + Send + Sync + 'static> FanoutBuilder<S> {
    /// Sets the Node that runs first and fans out to the parallel Nodes.
    pub fn from<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = S> + Send + 'static,
    {
        self.source = Some((name.to_string(), Arc::new(move |s| Box::pin(f(s)))));
        self
    }

    /// Adds a parallel plain-function Node. Declaration order decides overwrite and
    /// append order.
    pub fn node<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = S> + Send + 'static,
    {
        self.nodes
            .push((name.to_string(), Arc::new(move |s| Box::pin(f(s)))));
        self
    }

    /// Validates and builds the Fanout.
    pub fn build(self) -> Result<Fanout<S>, FanoutError> {
        if self.nodes.is_empty() {
            return Err(FanoutError::NoNodes);
        }
        Ok(Fanout {
            inner: Arc::new(Inner {
                rules: self.rules,
                source: self.source,
                nodes: self.nodes,
            }),
        })
    }
}

struct Inner<S> {
    rules: MergeRules<S>,
    source: Option<(String, NodeFn<S>)>,
    nodes: Vec<(String, NodeFn<S>)>,
}

/// An optional source Node followed by Nodes that run in parallel and merge into one State.
pub struct Fanout<S> {
    inner: Arc<Inner<S>>,
}

enum Stage<S> {
    Begin(S),
    SourceStart(S),
    SourceRun(S),
    BranchesStart(S),
    BranchesRun(S),
    Done,
}

struct Run<S> {
    queue: VecDeque<JobEvent<S>>,
    stage: Stage<S>,
}

impl<S: Clone + Send + Sync + 'static> Fanout<S> {
    /// Starts building a Fanout with the given merge rules.
    pub fn builder(rules: MergeRules<S>) -> FanoutBuilder<S> {
        FanoutBuilder {
            rules,
            source: None,
            nodes: Vec::new(),
        }
    }

    /// Runs a Job from `initial` and streams its events; the last one carries the final State.
    pub fn stream(&self, initial: S) -> BoxStream<'static, JobEvent<S>> {
        let inner = self.inner.clone();
        let run = Run {
            queue: VecDeque::new(),
            stage: Stage::Begin(initial),
        };
        stream::unfold(run, move |mut run| {
            let inner = inner.clone();
            async move {
                loop {
                    if let Some(event) = run.queue.pop_front() {
                        return Some((event, run));
                    }
                    match std::mem::replace(&mut run.stage, Stage::Done) {
                        Stage::Begin(s) => {
                            run.queue.push_back(JobEvent::JobStarted);
                            run.stage = Stage::SourceStart(s);
                        }
                        Stage::SourceStart(s) => match &inner.source {
                            Some((name, _)) => {
                                run.queue
                                    .push_back(JobEvent::NodeStarted { node: name.clone() });
                                run.stage = Stage::SourceRun(s);
                            }
                            None => run.stage = Stage::BranchesStart(s),
                        },
                        Stage::SourceRun(s) => {
                            let (name, f) =
                                inner.source.as_ref().expect("source stage has a source");
                            let updated = f(s).await;
                            run.queue
                                .push_back(JobEvent::NodeFinished { node: name.clone() });
                            run.stage = Stage::BranchesStart(updated);
                        }
                        Stage::BranchesStart(s) => {
                            for (name, _) in &inner.nodes {
                                run.queue
                                    .push_back(JobEvent::NodeStarted { node: name.clone() });
                            }
                            run.stage = Stage::BranchesRun(s);
                        }
                        Stage::BranchesRun(base) => {
                            let results =
                                join_all(inner.nodes.iter().map(|(_, f)| f(base.clone()))).await;
                            let names: Vec<String> =
                                inner.nodes.iter().map(|(n, _)| n.clone()).collect();
                            for node in &names {
                                run.queue
                                    .push_back(JobEvent::NodeFinished { node: node.clone() });
                            }
                            let (merged, conflicts) = inner.rules.merge(&base, results, &names);
                            for c in conflicts {
                                run.queue.push_back(JobEvent::Conflict {
                                    field: c.field,
                                    nodes: c.nodes,
                                });
                            }
                            run.queue.push_back(JobEvent::JobFinished { state: merged });
                        }
                        Stage::Done => return None,
                    }
                }
            }
        })
        .boxed()
    }

    /// Runs a Job from `initial` to the end and returns the final State.
    pub async fn run(&self, initial: S) -> Result<S, FanoutError> {
        let mut events = self.stream(initial);
        while let Some(event) = events.next().await {
            if let JobEvent::JobFinished { state } = event {
                return Ok(state);
            }
        }
        unreachable!("a Job stream always ends with JobFinished")
    }
}
