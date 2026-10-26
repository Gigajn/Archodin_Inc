//! Archodin Graph orchestration (ADR 0002).
//!
//! A [`Graph`] is built from Nodes that all read and update one typed State.
//! Running it for a person's request is a Job: [`Graph::stream`] yields the
//! [`JobEvent`]s of the run in order, and [`Graph::run`] returns the final State.
//!
//! Nodes are joined by Edges, some conditional (they pick the next Node from the
//! State). A Job stops at the Step limit (default 25, settable per run).
//! Durable Jobs write Checkpoints, resume, and can pause for approval.

mod durable;

pub use durable::{
    ApprovalDecision, ApprovalRequest, Approver, Checkpoint, CheckpointStore,
    InMemoryCheckpointStore, JobOutcome, ScriptedApprover,
};

mod budget;

pub use budget::{Budget, BudgetApprover, BudgetScope, Pricing, Usage, UsageMeter};

mod queue;

pub use queue::{
    Clock, JobId, JobOrigin, JobQueue, ManualClock, QueuedJob, ScheduleError, ScheduleId,
    Scheduler, SystemClock, Timetable,
};

mod parallel;

pub use parallel::{Fanout, FanoutBuilder, FanoutError, MergeRules};

mod panel;

pub use panel::{Panel, PanelError, PanelResult, Qadi, QadiError};

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};

mod explorador;
pub use explorador::{
    Charter, Contribution, Department, Explorador, ExploradorError, GitHub, GitHubCall,
    GitHubClient, GitHubError, Issue, Opportunity, PullRequest, PullRequestDraft, RepoInfo,
    RepoRef, ScriptedGitHub,
};

mod nakama;
pub use nakama::{Nakama, Proposal, Workload};

mod performance;
pub use performance::{AgentFigures, CitedProposal, PerformanceLedger, PerformanceReport};

mod roster;
pub use roster::{branch_name, Agent, Role, Roster, RosterError};

mod vault;
pub use vault::{Secret, Vault, VaultError};

mod agent;
mod chain;
mod gemini;
mod native;
pub use agent::{AgentError, AgentNode, AgentTool, Provider, TypedTool};
pub use chain::{ChainEvent, FallbackReason, ModelChain, ModelPair};
pub use gemini::Gemini;

mod strategos;
pub use strategos::{Assignment, Piece, Strategos, StrategosError, StrategosEvent};

type NodeFn<S> = Arc<dyn Fn(S) -> BoxFuture<'static, S> + Send + Sync>;
type RouterFn<S> = Arc<dyn Fn(&S) -> &'static str + Send + Sync>;

/// The Step limit used when a run does not set one.
pub const DEFAULT_STEP_LIMIT: usize = 25;

/// Edge target meaning "the Job ends here".
pub const END: &str = "__end__";

enum Edge<S> {
    To(String),
    Conditional {
        router: RouterFn<S>,
        targets: Vec<String>,
    },
}

/// Why a Graph could not be built, or a Job could not finish.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// No entry Node was set.
    #[error("the graph has no entry node")]
    NoEntry,
    /// The entry Node name does not match any Node.
    #[error("unknown entry node `{0}`")]
    UnknownEntry(String),
    /// An Edge starts or ends at a Node that does not exist.
    #[error("edge refers to unknown node `{0}`")]
    UnknownEdgeNode(String),
    /// A Node has more than one outgoing Edge.
    #[error("node `{0}` has more than one outgoing edge")]
    DuplicateEdge(String),
    /// The Job was stopped after `limit` Node executions with work still pending.
    #[error("step limit reached after {limit} steps")]
    StepLimitReached { limit: usize },
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
    /// Parallel Nodes wrote competing values to a decision field; it is left for the Qadi.
    Conflict { field: String, nodes: Vec<String> },
    /// The Qadi settled a conflict: `node`'s `value` (shown with `Debug`) became the
    /// value of `field`.
    Decision {
        field: String,
        node: String,
        value: String,
    },
    /// The Job ended; `state` is the final State.
    JobFinished { state: S },
    /// A durable Job paused: a step is waiting for a person's approval.
    JobPaused {
        node: String,
        prompt: String,
        state: S,
    },
    /// A durable Job stopped because a person rejected a step.
    JobRejected { node: String, state: S },
    /// The Job stopped because it hit the Step limit; `state` is the State so far.
    StepLimitReached { limit: usize, state: S },
}

/// Settings for one run, such as its Step limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    step_limit: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            step_limit: DEFAULT_STEP_LIMIT,
        }
    }
}

impl RunOptions {
    /// Options with the default Step limit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the most Node executions the run may take.
    pub fn step_limit(mut self, limit: usize) -> Self {
        self.step_limit = limit;
        self
    }
}

/// Builds a [`Graph`].
pub struct GraphBuilder<S> {
    nodes: HashMap<String, NodeFn<S>>,
    edges: HashMap<String, Edge<S>>,
    edge_order: Vec<String>,
    duplicate_edge: Option<String>,
    entry: Option<String>,
    approvals: HashMap<String, durable::PromptFn<S>>,
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

    /// Adds an Edge from `from` to `to`; `to` may be [`END`].
    pub fn edge(self, from: &str, to: &str) -> Self {
        self.add_edge(from, Edge::To(to.to_string()))
    }

    /// Adds a conditional Edge: after `from`, `router` reads the State and names the
    /// next Node. `targets` lists every name the router may return ([`END`] included).
    ///
    /// # Panics
    /// A run panics if `router` returns a name not listed in `targets`.
    pub fn conditional_edge<R>(self, from: &str, router: R, targets: &[&str]) -> Self
    where
        R: Fn(&S) -> &'static str + Send + Sync + 'static,
    {
        self.add_edge(
            from,
            Edge::Conditional {
                router: Arc::new(router),
                targets: targets.iter().map(|t| t.to_string()).collect(),
            },
        )
    }

    fn add_edge(mut self, from: &str, edge: Edge<S>) -> Self {
        if self.edges.insert(from.to_string(), edge).is_some() {
            self.duplicate_edge.get_or_insert(from.to_string());
        }
        self.edge_order.push(from.to_string());
        self
    }

    /// Validates and builds the Graph.
    pub fn build(self) -> Result<Graph<S>, GraphError> {
        let entry = self.entry.ok_or(GraphError::NoEntry)?;
        if !self.nodes.contains_key(&entry) {
            return Err(GraphError::UnknownEntry(entry));
        }
        if let Some(dup) = self.duplicate_edge {
            return Err(GraphError::DuplicateEdge(dup));
        }
        let known = |n: &str| n == END || self.nodes.contains_key(n);
        for from in &self.edge_order {
            if !self.nodes.contains_key(from) {
                return Err(GraphError::UnknownEdgeNode(from.clone()));
            }
            let targets: &[String] = match &self.edges[from] {
                Edge::To(t) => std::slice::from_ref(t),
                Edge::Conditional { targets, .. } => targets,
            };
            if let Some(bad) = targets.iter().find(|t| !known(t)) {
                return Err(GraphError::UnknownEdgeNode(bad.clone()));
            }
        }
        Ok(Graph {
            inner: Arc::new(Inner {
                nodes: self.nodes,
                edges: self.edges,
                entry,
                approvals: self.approvals,
            }),
        })
    }
}

struct Inner<S> {
    nodes: HashMap<String, NodeFn<S>>,
    edges: HashMap<String, Edge<S>>,
    entry: String,
    approvals: HashMap<String, durable::PromptFn<S>>,
}

/// Nodes over one shared State, ready to run Jobs.
pub struct Graph<S> {
    inner: Arc<Inner<S>>,
}

enum Phase<S> {
    Start(S),
    Enter(S, String),
    Exec(S, String),
    Leave(S, String),
    Finish(S),
    Done,
}

struct Run<S> {
    phase: Phase<S>,
    steps: usize,
}

impl<S: Clone + Send + 'static> Graph<S> {
    /// Starts building a Graph.
    pub fn builder() -> GraphBuilder<S> {
        GraphBuilder {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            edge_order: Vec::new(),
            duplicate_edge: None,
            entry: None,
            approvals: HashMap::new(),
        }
    }

    /// Runs a Job from `initial` with the default Step limit and streams its events.
    pub fn stream(&self, initial: S) -> BoxStream<'static, JobEvent<S>> {
        self.stream_with(initial, RunOptions::default())
    }

    /// Runs a Job from `initial` with `options` and streams its events; the last one
    /// is [`JobEvent::JobFinished`] or [`JobEvent::StepLimitReached`].
    pub fn stream_with(&self, initial: S, options: RunOptions) -> BoxStream<'static, JobEvent<S>> {
        let inner = self.inner.clone();
        let limit = options.step_limit;
        let run = Run {
            phase: Phase::Start(initial),
            steps: 0,
        };
        stream::unfold(run, move |run| {
            let inner = inner.clone();
            async move {
                let Run { phase, steps } = run;
                let (event, phase, steps) = match phase {
                    Phase::Start(s) => (
                        Some(JobEvent::JobStarted),
                        Phase::Enter(s, inner.entry.clone()),
                        steps,
                    ),
                    Phase::Enter(s, _) if steps >= limit => (
                        Some(JobEvent::StepLimitReached { limit, state: s }),
                        Phase::Done,
                        steps,
                    ),
                    Phase::Enter(s, node) => (
                        Some(JobEvent::NodeStarted { node: node.clone() }),
                        Phase::Exec(s, node),
                        steps + 1,
                    ),
                    Phase::Exec(s, node) => {
                        let updated = (inner.nodes[&node])(s).await;
                        (
                            Some(JobEvent::NodeFinished { node: node.clone() }),
                            Phase::Leave(updated, node),
                            steps,
                        )
                    }
                    Phase::Leave(s, node) => {
                        let next = match inner.edges.get(&node) {
                            None => END,
                            Some(Edge::To(t)) => t.as_str(),
                            Some(Edge::Conditional { router, targets }) => {
                                let chosen = router(&s);
                                assert!(
                                    targets.iter().any(|t| t == chosen),
                                    "conditional edge from `{node}` returned undeclared target `{chosen}`"
                                );
                                chosen
                            }
                        };
                        let phase = if next == END {
                            Phase::Finish(s)
                        } else {
                            Phase::Enter(s, next.to_string())
                        };
                        (None, phase, steps)
                    }
                    Phase::Finish(s) => (
                        Some(JobEvent::JobFinished { state: s }),
                        Phase::Done,
                        steps,
                    ),
                    Phase::Done => return None,
                };
                Some((event, Run { phase, steps }))
            }
        })
        .filter_map(|e| async move { e })
        .boxed()
    }

    /// Runs a Job with the default Step limit and returns the final State.
    pub async fn run(&self, initial: S) -> Result<S, GraphError> {
        self.run_with(initial, RunOptions::default()).await
    }

    /// Runs a Job with `options`; a Job that hits the Step limit returns
    /// [`GraphError::StepLimitReached`].
    pub async fn run_with(&self, initial: S, options: RunOptions) -> Result<S, GraphError> {
        let mut events = self.stream_with(initial, options);
        while let Some(event) = events.next().await {
            match event {
                JobEvent::JobFinished { state } => return Ok(state),
                JobEvent::StepLimitReached { limit, .. } => {
                    return Err(GraphError::StepLimitReached { limit })
                }
                _ => {}
            }
        }
        unreachable!("a Job stream always ends with JobFinished or StepLimitReached")
    }
}

mod handoff;
pub use handoff::{Briefing, Handoff, Turn, HISTORY_TOOL};
mod skills;
pub use skills::{
    FlowRun, PendingSkill, Skill, SkillEntryRecord, SkillError, SkillKind, SkillLibrary,
    SkillSnapshot, SkillVersionRecord,
};
mod learning;
pub use learning::{fact_key, Akasha, LearnError, Learned, Learner, MIN_TOOL_CALLS};
mod vigil;
pub use vigil::{RejectedBy, SkillSource, Verdict, Vigil};
mod sandbox;
pub use sandbox::{
    run_skill_script, ContainerSandbox, Limit, Sandbox, SandboxError, SandboxInput, SandboxOutput,
    SandboxPolicy, SandboxTool, ScriptedSandbox, SkillScriptError,
};
mod mcp;
pub use mcp::{McpClient, McpError, McpLibrary, McpServer, PendingMcpServer};
mod ronin;
pub use ronin::{
    web_search, DuckDuckGoSearch, FoundSkill, GitHubSearch, Ronin, RoninError, Scouting,
    SearchEngine, SearchHit, SearxngSearch,
};

mod upgrade;
pub use upgrade::{
    DepartmentSetup, RunningJob, Upgrade, UpgradeCharter, UpgradeError, VersionedDepartment,
};
mod forja;
pub use forja::{Forja, ForjaError, ForjaGitHub, ForjaPullRequest, ForjaWork};
