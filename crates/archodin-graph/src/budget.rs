//! Token and cost accounting, and Budgets that stop and ask.
//!
//! A [`UsageMeter`] records the [`Usage`] of every metered step, per Agent and per
//! Job. A [`Budget`] is the most an Agent or a Job may spend. A Node added with
//! [`GraphBuilder::metered_node`] reports its usage and, before it runs, asks the
//! approval interface whether to go on if its Job's or Agent's Budget is already
//! exceeded. The Job therefore pauses at the Checkpoint written after the step
//! that went over, continues if a person approves the overrun and stops if they
//! reject it. Wrap the real [`Approver`] in a [`BudgetApprover`] so that steps
//! within their Budgets are never asked about.
//!
//! The meter lives in memory: usage recorded before a process restart is not
//! restored when a Job resumes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use crate::{ApprovalDecision, ApprovalRequest, Approver, GraphBuilder};

/// Tokens used and money spent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    /// Tokens sent to the model.
    pub prompt_tokens: u64,
    /// Tokens the model produced.
    pub completion_tokens: u64,
    /// Money spent, in the currency the [`Pricing`] uses (for example dollars).
    pub cost: f64,
}

impl Usage {
    /// Usage of `prompt_tokens` and `completion_tokens` that cost `cost`.
    pub fn new(prompt_tokens: u64, completion_tokens: u64, cost: f64) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            cost,
        }
    }

    /// Prompt and completion tokens together.
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }

    fn add(&mut self, other: Usage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.cost += other.cost;
    }
}

/// What a Provider account charges, used to turn tokens into cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pricing {
    prompt_per_million: f64,
    completion_per_million: f64,
}

impl Pricing {
    /// A price per million prompt tokens and per million completion tokens.
    pub fn per_million(prompt: f64, completion: f64) -> Self {
        Self {
            prompt_per_million: prompt,
            completion_per_million: completion,
        }
    }

    /// `usage` with its cost set from its tokens.
    pub fn price(&self, usage: Usage) -> Usage {
        Usage {
            cost: usage.prompt_tokens as f64 * self.prompt_per_million / 1_000_000.0
                + usage.completion_tokens as f64 * self.completion_per_million / 1_000_000.0,
            ..usage
        }
    }
}

/// The most tokens or money an Agent or a Job may spend before it asks a person.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Budget {
    max_tokens: Option<u64>,
    max_cost: Option<f64>,
}

impl Budget {
    /// A Budget of `max` tokens.
    pub fn tokens(max: u64) -> Self {
        Self {
            max_tokens: Some(max),
            max_cost: None,
        }
    }

    /// A Budget of `max` money.
    pub fn cost(max: f64) -> Self {
        Self {
            max_tokens: None,
            max_cost: Some(max),
        }
    }

    /// Also limits tokens.
    pub fn with_tokens(mut self, max: u64) -> Self {
        self.max_tokens = Some(max);
        self
    }

    /// Also limits money.
    pub fn with_cost(mut self, max: f64) -> Self {
        self.max_cost = Some(max);
        self
    }

    fn exceeded_by(&self, used: &Usage) -> bool {
        self.max_tokens.is_some_and(|m| used.total_tokens() > m)
            || self.max_cost.is_some_and(|m| used.cost > m)
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(m) = self.max_tokens {
            parts.push(format!("{m} tokens"));
        }
        if let Some(m) = self.max_cost {
            parts.push(format!("{m:.4} cost"));
        }
        parts.join(" and ")
    }
}

/// Whose Budget is exceeded.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BudgetScope {
    /// A Job's Budget.
    Job(String),
    /// An Agent's Budget.
    Agent(String),
}

#[derive(Default)]
struct MeterState {
    by_job: HashMap<String, Usage>,
    by_agent: HashMap<String, Usage>,
    job_budgets: HashMap<String, Budget>,
    agent_budgets: HashMap<String, Budget>,
    waived: Vec<BudgetScope>,
    node_agents: HashMap<String, String>,
}

/// Records usage per Agent and per Job and holds their Budgets.
#[derive(Default)]
pub struct UsageMeter {
    state: Mutex<MeterState>,
}

impl UsageMeter {
    /// An empty meter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `agent` used `usage` working on `job`.
    pub fn record(&self, job: &str, agent: &str, usage: Usage) {
        let mut s = self.state.lock().unwrap();
        s.by_job.entry(job.to_string()).or_default().add(usage);
        s.by_agent.entry(agent.to_string()).or_default().add(usage);
    }

    /// Everything `job` has used so far.
    pub fn job_usage(&self, job: &str) -> Usage {
        let s = self.state.lock().unwrap();
        s.by_job.get(job).copied().unwrap_or_default()
    }

    /// Everything `agent` has used so far, across Jobs.
    pub fn agent_usage(&self, agent: &str) -> Usage {
        let s = self.state.lock().unwrap();
        s.by_agent.get(agent).copied().unwrap_or_default()
    }

    /// Sets the Budget of a Job.
    pub fn set_job_budget(&self, job: &str, budget: Budget) {
        let mut s = self.state.lock().unwrap();
        s.job_budgets.insert(job.to_string(), budget);
        s.waived.retain(|w| *w != BudgetScope::Job(job.to_string()));
    }

    /// Sets the Budget of an Agent, counted across all its Jobs.
    pub fn set_agent_budget(&self, agent: &str, budget: Budget) {
        let mut s = self.state.lock().unwrap();
        s.agent_budgets.insert(agent.to_string(), budget);
        s.waived
            .retain(|w| *w != BudgetScope::Agent(agent.to_string()));
    }

    /// The first exceeded Budget, Job before Agent, that has not had its overrun approved.
    pub fn exceeded(&self, job: &str, agent: &str) -> Option<BudgetScope> {
        self.exceeded_with_reason(job, agent)
            .map(|(scope, _)| scope)
    }

    fn exceeded_with_reason(&self, job: &str, agent: &str) -> Option<(BudgetScope, String)> {
        let s = self.state.lock().unwrap();
        let checks = [
            (
                BudgetScope::Job(job.to_string()),
                s.job_budgets.get(job).copied(),
                s.by_job.get(job).copied().unwrap_or_default(),
                format!("Job `{job}`"),
            ),
            (
                BudgetScope::Agent(agent.to_string()),
                s.agent_budgets.get(agent).copied(),
                s.by_agent.get(agent).copied().unwrap_or_default(),
                format!("Agent `{agent}`"),
            ),
        ];
        checks.into_iter().find_map(|(scope, budget, used, who)| {
            let budget = budget?;
            (budget.exceeded_by(&used) && !s.waived.contains(&scope)).then(|| {
                let reason = format!(
                    "{who} exceeded its Budget: used {} tokens (cost {:.4}) of {}. Approve the overrun?",
                    used.total_tokens(),
                    used.cost,
                    budget.describe()
                );
                (scope, reason)
            })
        })
    }

    /// Records that a person approved the overrun of this Budget: it asks no more.
    pub fn approve_overrun(&self, scope: BudgetScope) {
        let mut s = self.state.lock().unwrap();
        if !s.waived.contains(&scope) {
            s.waived.push(scope);
        }
    }

    fn register_node(&self, node: &str, agent: &str) {
        self.state
            .lock()
            .unwrap()
            .node_agents
            .insert(node.to_string(), agent.to_string());
    }

    fn agent_of(&self, node: &str) -> Option<String> {
        self.state.lock().unwrap().node_agents.get(node).cloned()
    }
}

/// An [`Approver`] that answers for metered steps within their Budgets and hands
/// the questions about exceeded Budgets to the real approval interface.
pub struct BudgetApprover<A> {
    meter: Arc<UsageMeter>,
    inner: A,
}

impl<A: Approver> BudgetApprover<A> {
    /// Wraps `inner`, which a person answers through.
    pub fn new(meter: Arc<UsageMeter>, inner: A) -> Self {
        Self { meter, inner }
    }

    /// The wrapped approval interface.
    pub fn inner(&self) -> &A {
        &self.inner
    }
}

impl<A: Approver> Approver for BudgetApprover<A> {
    fn decide(&self, request: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision> {
        let request = request.clone();
        Box::pin(async move {
            let Some(agent) = self.meter.agent_of(&request.node) else {
                return self.inner.decide(&request).await;
            };
            let Some(scope) = self.meter.exceeded(&request.job, &agent) else {
                return ApprovalDecision::Approved;
            };
            let decision = self.inner.decide(&request).await;
            if decision == ApprovalDecision::Approved {
                self.meter.approve_overrun(scope);
            }
            decision
        })
    }
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a Node worked by `agent` whose usage is recorded in `meter` against the
    /// Job named by `job_of`. Before it runs, an exceeded Budget of that Job or Agent
    /// pauses the Job and asks for a decision (see [`BudgetApprover`]). `f` returns
    /// the updated State and what the step used.
    pub fn metered_node<J, F, Fut>(
        self,
        name: &str,
        agent: &str,
        meter: Arc<UsageMeter>,
        job_of: J,
        f: F,
    ) -> Self
    where
        J: Fn(&S) -> String + Send + Sync + 'static,
        F: Fn(S) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = (S, Usage)> + Send + 'static,
    {
        meter.register_node(name, agent);
        let job_of = Arc::new(job_of);
        let f = Arc::new(f);
        let agent = agent.to_string();
        let prompt = {
            let (meter, job_of, agent) = (meter.clone(), job_of.clone(), agent.clone());
            move |s: &S| {
                meter
                    .exceeded_with_reason(&job_of(s), &agent)
                    .map(|(_, reason)| reason)
                    .unwrap_or_else(|| "within Budget".to_string())
            }
        };
        self.node_with_approval(name, prompt, move |s: S| {
            let (meter, job_of, f, agent) =
                (meter.clone(), job_of.clone(), f.clone(), agent.clone());
            async move {
                let job = job_of(&s);
                let (updated, usage) = f(s).await;
                meter.record(&job, &agent, usage);
                updated
            }
        })
    }
}
