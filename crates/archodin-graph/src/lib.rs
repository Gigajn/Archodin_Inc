//! Archodin Graph orchestration (ADR 0002).
//!
//! A [`Graph`] is built from [`Node`]s that all read and update one typed State.
//! Running it for a person's request is a Job: [`Graph::stream`] yields the
//! [`JobEvent`]s of the run in order, and [`Graph::run`] returns the final State.
//!
//! This first slice supports a single plain-function Node; Edges, the Step limit
//! and Checkpoints arrive with later tickets.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};

type NodeFn<S> = Arc<dyn Fn(S) -> BoxFuture<'static, S> + Send + Sync>;

/// Why a Graph could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// No entry Node was set.
    #[error("the graph has no entry node")]
    NoEntry,
    /// The entry Node name does not match any Node.
    #[error("unknown entry node `{0}`")]
    UnknownEntry(String),
}

/// What happened during a Job, in the order it happened.
#[derive(Debug, Clone, PartialEq)]
pub enum JobEvent<S> {
    /// The Job began.
    JobStarted,
    /// A Node began executing.
    NodeStarted { node: String },
    /// A Node finished and its update was applied to the State.
    NodeFinished { node: String },
    /// The Job ended; `state` is the final State.
    JobFinished { state: S },
}

/// Builds a [`Graph`].
pub struct GraphBuilder<S> {
    nodes: HashMap<String, NodeFn<S>>,
    entry: Option<String>,
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a plain-function Node: it receives the State and returns the updated State.
    pub fn node<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = S> + Send + 'static,
    {
        self.nodes
            .insert(name.to_string(), Arc::new(move |s| Box::pin(f(s))));
        self
    }

    /// Sets the Node a Job starts at.
    pub fn entry(mut self, name: &str) -> Self {
        self.entry = Some(name.to_string());
        self
    }

    /// Validates and builds the Graph.
    pub fn build(self) -> Result<Graph<S>, GraphError> {
        let entry = self.entry.ok_or(GraphError::NoEntry)?;
        if !self.nodes.contains_key(&entry) {
            return Err(GraphError::UnknownEntry(entry));
        }
        Ok(Graph {
            nodes: self.nodes,
            entry,
        })
    }
}

/// Nodes over one shared State, ready to run Jobs.
pub struct Graph<S> {
    nodes: HashMap<String, NodeFn<S>>,
    entry: String,
}

enum Phase<S> {
    Start(S),
    NodeStart(S),
    NodeEnd(S),
    Finish(S),
    Done,
}

impl<S: Clone + Send + 'static> Graph<S> {
    /// Starts building a Graph.
    pub fn builder() -> GraphBuilder<S> {
        GraphBuilder {
            nodes: HashMap::new(),
            entry: None,
        }
    }

    /// Runs a Job from `initial` and streams its events; the last one carries the final State.
    pub fn stream(&self, initial: S) -> BoxStream<'static, JobEvent<S>> {
        let node_name = self.entry.clone();
        let node = self.nodes[&self.entry].clone();
        stream::unfold(Phase::Start(initial), move |phase| {
            let node_name = node_name.clone();
            let node = node.clone();
            async move {
                match phase {
                    Phase::Start(s) => Some((JobEvent::JobStarted, Phase::NodeStart(s))),
                    Phase::NodeStart(s) => {
                        Some((JobEvent::NodeStarted { node: node_name }, Phase::NodeEnd(s)))
                    }
                    Phase::NodeEnd(s) => {
                        let updated = node(s).await;
                        Some((
                            JobEvent::NodeFinished { node: node_name },
                            Phase::Finish(updated),
                        ))
                    }
                    Phase::Finish(s) => Some((JobEvent::JobFinished { state: s }, Phase::Done)),
                    Phase::Done => None,
                }
            }
        })
        .boxed()
    }

    /// Runs a Job from `initial` to the end and returns the final State.
    pub async fn run(&self, initial: S) -> Result<S, GraphError> {
        let mut events = self.stream(initial);
        while let Some(event) = events.next().await {
            if let JobEvent::JobFinished { state } = event {
                return Ok(state);
            }
        }
        unreachable!("a Job stream always ends with JobFinished")
    }
}
