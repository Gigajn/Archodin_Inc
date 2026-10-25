//! The Strategos: the Agent that splits a Job into pieces and hands each to a named
//! member as a Flow, and Graphs that run as Nodes so Departments nest.
//!
//! A [`Strategos`] asks its planner Agent to split the Job, then runs every piece with
//! the member it names: an Agent ([`Strategos::agent`]) or a whole nested Department
//! ([`Strategos::department`]), which is how a Strategos leads other Strategoi.
//! [`GraphBuilder::strategos_node`] runs it as a Node; [`GraphBuilder::graph_node`]
//! runs any whole Graph as a Node, sharing results through the State.

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;

use crate::{AgentError, AgentNode, Graph, GraphBuilder, GraphError};

type MemberFn = Arc<dyn Fn(String) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;
type Observer = Arc<dyn Fn(StrategosEvent) + Send + Sync>;

/// One piece of a Job, assigned to a named member.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Piece {
    /// The member (Agent or nested Department) that handles the piece.
    pub agent: String,
    /// What that member is asked to do.
    pub task: String,
}

/// A piece together with what its member returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub piece: Piece,
    pub result: String,
}

/// Which member handled which piece, as it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategosEvent {
    /// A piece was handed to a member.
    PieceAssigned {
        strategos: String,
        agent: String,
        task: String,
    },
    /// The member finished the piece.
    PieceFinished { strategos: String, agent: String },
}

/// Why a Strategos could not finish a Job.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrategosError {
    /// The planner Agent failed.
    #[error("the strategos could not plan: {0}")]
    Planner(AgentError),
    /// The plan was not a JSON list of `{agent, task}` pieces.
    #[error("the plan is not a list of pieces: {0}")]
    BadPlan(String),
    /// A piece names a member the Strategos does not lead.
    #[error("piece assigned to unknown member `{0}`")]
    UnknownMember(String),
    /// A member failed its piece.
    #[error("member `{agent}` failed: {reason}")]
    PieceFailed { agent: String, reason: String },
}

/// A supervisor that splits a Job and hands the pieces to its members.
pub struct Strategos {
    name: String,
    planner: AgentNode,
    members: Vec<(String, MemberFn)>,
    observer: Option<Observer>,
}

impl Strategos {
    /// A Strategos called `name`; `planner` is the Agent that splits Jobs.
    pub fn new(name: impl Into<String>, planner: AgentNode) -> Self {
        Self {
            name: name.into(),
            planner,
            members: Vec::new(),
            observer: None,
        }
    }

    /// Adds an Agent the Strategos can hand pieces to.
    pub fn agent(mut self, name: &str, agent: AgentNode) -> Self {
        let agent = Arc::new(agent);
        self.members.push((
            name.to_string(),
            Arc::new(move |task| {
                let agent = agent.clone();
                Box::pin(async move { agent.answer(&task).await.map_err(|e| e.to_string()) })
            }),
        ));
        self
    }

    /// Adds a nested Department: a whole Graph (typically led by another Strategos).
    /// `input` turns the piece's task into the Graph's State, `output` reads the
    /// result back from its final State.
    pub fn department<T, I, O>(mut self, name: &str, graph: Graph<T>, input: I, output: O) -> Self
    where
        T: Clone + Send + 'static,
        I: Fn(&str) -> T + Send + Sync + 'static,
        O: Fn(T) -> String + Send + Sync + 'static,
    {
        let (graph, input, output) = (Arc::new(graph), Arc::new(input), Arc::new(output));
        self.members.push((
            name.to_string(),
            Arc::new(move |task| {
                let (graph, input, output) = (graph.clone(), input.clone(), output.clone());
                Box::pin(async move {
                    let state = graph.run(input(&task)).await.map_err(|e| e.to_string())?;
                    Ok(output(state))
                })
            }),
        ));
        self
    }

    /// Reports each piece assignment and completion to `observer`.
    pub fn observe<F: Fn(StrategosEvent) + Send + Sync + 'static>(mut self, observer: F) -> Self {
        self.observer = Some(Arc::new(observer));
        self
    }

    fn emit(&self, event: StrategosEvent) {
        if let Some(observer) = &self.observer {
            observer(event);
        }
    }

    /// Splits `job` and runs every piece, in order, with the member it names.
    pub async fn handle(&self, job: &str) -> Result<Vec<Assignment>, StrategosError> {
        let names: Vec<&str> = self.members.iter().map(|(n, _)| n.as_str()).collect();
        let request = format!(
            "Job: {job}\nMembers: {}\nSplit the Job into pieces. Reply with only a JSON list \
             of objects {{\"agent\": <member>, \"task\": <what to do>}}.",
            names.join(", ")
        );
        let plan = self
            .planner
            .answer(&request)
            .await
            .map_err(StrategosError::Planner)?;
        let pieces = parse_plan(&plan)?;
        let by_name: HashMap<&str, &MemberFn> =
            self.members.iter().map(|(n, f)| (n.as_str(), f)).collect();
        if let Some(bad) = pieces
            .iter()
            .find(|p| !by_name.contains_key(p.agent.as_str()))
        {
            return Err(StrategosError::UnknownMember(bad.agent.clone()));
        }
        let mut done = Vec::new();
        for piece in pieces {
            self.emit(StrategosEvent::PieceAssigned {
                strategos: self.name.clone(),
                agent: piece.agent.clone(),
                task: piece.task.clone(),
            });
            let result = by_name[piece.agent.as_str()](piece.task.clone())
                .await
                .map_err(|reason| StrategosError::PieceFailed {
                    agent: piece.agent.clone(),
                    reason,
                })?;
            self.emit(StrategosEvent::PieceFinished {
                strategos: self.name.clone(),
                agent: piece.agent.clone(),
            });
            done.push(Assignment { piece, result });
        }
        Ok(done)
    }
}

fn parse_plan(plan: &str) -> Result<Vec<Piece>, StrategosError> {
    let (start, end) = plan
        .find('[')
        .zip(plan.rfind(']'))
        .filter(|(s, e)| s < e)
        .ok_or_else(|| StrategosError::BadPlan(plan.to_string()))?;
    serde_json::from_str(&plan[start..=end]).map_err(|e| StrategosError::BadPlan(e.to_string()))
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a Strategos Node: `read` takes the Job from the State, and `write` puts
    /// the handled pieces (or the failure) back into it.
    pub fn strategos_node<R, W>(self, name: &str, strategos: Strategos, read: R, write: W) -> Self
    where
        R: Fn(&S) -> String + Send + Sync + 'static,
        W: Fn(S, Result<Vec<Assignment>, StrategosError>) -> S + Send + Sync + 'static,
    {
        let (strategos, read, write) = (Arc::new(strategos), Arc::new(read), Arc::new(write));
        self.node(name, move |state: S| {
            let (strategos, read, write) = (strategos.clone(), read.clone(), write.clone());
            async move {
                let handled = strategos.handle(&read(&state)).await;
                write(state, handled)
            }
        })
    }

    /// Adds a whole other Graph as a Node: `input` builds the inner State from this
    /// State, and `merge` folds the inner result (or the failure) back into it.
    pub fn graph_node<T, I, M>(self, name: &str, graph: Graph<T>, input: I, merge: M) -> Self
    where
        T: Clone + Send + 'static,
        I: Fn(&S) -> T + Send + Sync + 'static,
        M: Fn(S, Result<T, GraphError>) -> S + Send + Sync + 'static,
    {
        let (graph, input, merge) = (Arc::new(graph), Arc::new(input), Arc::new(merge));
        self.node(name, move |state: S| {
            let (graph, input, merge) = (graph.clone(), input.clone(), merge.clone());
            async move {
                let result = graph.run(input(&state)).await;
                merge(state, result)
            }
        })
    }
}
