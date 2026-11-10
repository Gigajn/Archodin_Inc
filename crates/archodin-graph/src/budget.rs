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

    pub(crate) fn add(&mut self, other: Usage) {
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

    fn set_agent_budget_if_unset(&self, agent: &str, budget: Budget) {
        let unset = !self.state.lock().unwrap().agent_budgets.contains_key(agent);
        if unset {
            self.set_agent_budget(agent, budget);
        }
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

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a Node run by a Roster [`Agent`](crate::Agent), metered like
    /// [`GraphBuilder::metered_node`] under the Agent's name. The usage is priced by
    /// the Model chain pair that answered. If the Agent's Role sets a token Budget
    /// and the meter has no Budget for the Agent yet, that Budget applies.
    pub fn metered_agent_node<J, R, W>(
        self,
        name: &str,
        agent: crate::Agent,
        meter: Arc<UsageMeter>,
        job_of: J,
        read: R,
        write: W,
    ) -> Self
    where
        J: Fn(&S) -> String + Send + Sync + 'static,
        R: Fn(&S) -> String + Send + Sync + 'static,
        W: Fn(S, Result<String, crate::AgentError>) -> S + Send + Sync + 'static,
    {
        if let Some(tokens) = agent.budget_tokens() {
            meter.set_agent_budget_if_unset(agent.name(), Budget::tokens(tokens));
        }
        let (read, write) = (Arc::new(read), Arc::new(write));
        let agent_name = agent.name().to_string();
        self.metered_node(name, &agent_name, meter, job_of, move |state: S| {
            let (agent, read, write) = (agent.clone(), read.clone(), write.clone());
            async move {
                match agent.ask_with_usage(&read(&state)).await {
                    Ok((answer, usage)) => (write(state, Ok(answer)), usage),
                    Err(e) => (write(state, Err(e)), Usage::default()),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// Runs an async block on a throwaway current-thread runtime.
    /// (`tokio::test` is unavailable: the `macros` feature of tokio is not
    /// enabled for this crate.)
    fn run_blocking<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(f)
    }

    fn over_budget_meter() -> UsageMeter {
        let meter = UsageMeter::new();
        meter.set_agent_budget("Lumen", Budget::tokens(100));
        meter.record("job-1", "Lumen", Usage::new(0, 150, 0.0));
        meter
    }

    #[test]
    fn waiver_is_single_use() {
        let meter = over_budget_meter();
        assert_eq!(
            meter.exceeded("job-1", "Lumen"),
            Some(BudgetScope::Agent("Lumen".into()))
        );
        meter.approve_overrun(BudgetScope::Agent("Lumen".into()), "tester");
        assert_eq!(meter.exceeded("job-1", "Lumen"), None);
        let reservation = meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .expect("the first over-budget reserve uses the waiver");
        reservation.settle(Usage::new(0, 10, 0.0));
        assert_eq!(
            meter.exceeded("job-1", "Lumen"),
            Some(BudgetScope::Agent("Lumen".into()))
        );
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .is_err());
    }

    #[test]
    fn waiver_expires() {
        let meter = over_budget_meter();
        meter.approve_overrun_with_ttl(
            BudgetScope::Agent("Lumen".into()),
            "tester",
            Duration::from_secs(0),
        );
        assert_eq!(
            meter.exceeded("job-1", "Lumen"),
            Some(BudgetScope::Agent("Lumen".into()))
        );
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .is_err());
        meter.approve_overrun_with_ttl(
            BudgetScope::Agent("Lumen".into()),
            "tester",
            Duration::from_secs(3600),
        );
        assert_eq!(meter.exceeded("job-1", "Lumen"), None);
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .is_ok());
    }

    #[test]
    fn concurrent_reserves_cannot_collectively_exceed_budget() {
        use std::sync::Barrier;
        let meter = Arc::new(UsageMeter::new());
        meter.set_agent_budget("Lumen", Budget::tokens(100));
        // 8 workers + the main thread meet at all three barriers.
        let start = Arc::new(Barrier::new(9));
        let reserved = Arc::new(Barrier::new(9));
        let checked = Arc::new(Barrier::new(9));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let meter = meter.clone();
                let start = start.clone();
                let reserved = reserved.clone();
                let checked = checked.clone();
                std::thread::spawn(move || {
                    start.wait();
                    let guard = meter.reserve("job-1", "Lumen", Usage::new(0, 60, 0.0)).ok();
                    let succeeded = guard.is_some();
                    // Hold the reservation until the main thread has checked
                    // the total, then drop it (refund) and report.
                    reserved.wait();
                    checked.wait();
                    drop(guard);
                    succeeded
                })
            })
            .collect();
        start.wait();
        // Every worker has reserved (or failed) by the time this returns.
        reserved.wait();
        assert_eq!(meter.agent_usage("Lumen").total_tokens(), 60);
        checked.wait();
        let succeeded = handles
            .into_iter()
            .map(|h| h.join().expect("worker thread"))
            .filter(|s| *s)
            .count();
        assert_eq!(
            succeeded, 1,
            "exactly one 60-token reserve fits in a 100-token budget"
        );
        assert_eq!(meter.agent_usage("Lumen").total_tokens(), 0);
    }

    #[test]
    fn pricing_rejects_nan_negative_and_infinite() {
        assert!(Pricing::per_million(f64::NAN, 1.0).is_err());
        assert!(Pricing::per_million(1.0, f64::NAN).is_err());
        assert!(Pricing::per_million(-1.0, 1.0).is_err());
        assert!(Pricing::per_million(1.0, -1.0).is_err());
        assert!(Pricing::per_million(f64::INFINITY, 1.0).is_err());
        assert!(Pricing::per_million(1.0, f64::NEG_INFINITY).is_err());
        assert!(Pricing::per_million(0.0, 0.0).is_ok());
        assert!(Pricing::per_million(2.0, 4.0).is_ok());
    }

    #[test]
    fn budget_cost_rejects_invalid() {
        assert!(Budget::cost(f64::NAN).is_err());
        assert!(Budget::cost(-1.0).is_err());
        assert!(Budget::cost(f64::INFINITY).is_err());
        assert!(Budget::cost(0.0).is_ok());
        assert!(Budget::tokens(100).with_cost(f64::NAN).is_err());
        assert!(Budget::tokens(100).with_cost(10.0).is_ok());
    }

    #[test]
    fn record_rejects_nan_and_negative() {
        let meter = UsageMeter::new();
        meter.record("job-1", "Lumen", Usage::new(10, 10, f64::NAN));
        meter.record("job-1", "Lumen", Usage::new(10, 10, -1.0));
        meter.record("job-1", "Lumen", Usage::new(10, 10, f64::INFINITY));
        assert_eq!(meter.job_usage("job-1"), Usage::default());
        meter.record("job-1", "Lumen", Usage::new(10, 10, 0.5));
        assert_eq!(meter.job_usage("job-1").total_tokens(), 20);
        assert!((meter.job_usage("job-1").cost - 0.5).abs() < 1e-9);
    }

    #[test]
    fn exceeded_by_is_fail_closed_on_nan() {
        let budget = Budget::cost(10.0).unwrap();
        assert!(budget.exceeded_by(&Usage::new(0, 0, f64::NAN)));
        let nan_budget = Budget {
            max_tokens: None,
            max_cost: Some(f64::NAN),
        };
        assert!(nan_budget.exceeded_by(&Usage::new(0, 0, 1.0)));
        assert!(!budget.exceeded_by(&Usage::new(0, 0, 1.0)));
    }

    #[test]
    fn settle_arithmetic_refund_and_top_up() {
        let meter = UsageMeter::new();
        meter.set_job_budget("job-1", Budget::tokens(1_000_000));
        let reservation = meter
            .reserve("job-1", "Lumen", Usage::new(40, 20, 1.0))
            .unwrap();
        reservation.settle(Usage::new(30, 10, 0.5));
        let used = meter.job_usage("job-1");
        assert_eq!(used.total_tokens(), 40);
        assert!((used.cost - 0.5).abs() < 1e-9);
        assert_eq!(meter.agent_usage("Lumen"), used);
        let reservation = meter
            .reserve("job-1", "Lumen", Usage::new(40, 20, 1.0))
            .unwrap();
        reservation.settle(Usage::new(70, 30, 2.0));
        let used = meter.job_usage("job-1");
        assert_eq!(used.total_tokens(), 140);
        assert!((used.cost - 2.5).abs() < 1e-9);
    }

    #[test]
    fn reservation_drop_refunds_estimate() {
        let meter = UsageMeter::new();
        {
            let _reservation = meter
                .reserve("job-1", "Lumen", Usage::new(40, 20, 1.0))
                .unwrap();
            assert_eq!(meter.job_usage("job-1").total_tokens(), 60);
            assert_eq!(meter.agent_usage("Lumen").total_tokens(), 60);
        }
        assert_eq!(meter.job_usage("job-1"), Usage::default());
        assert_eq!(meter.agent_usage("Lumen"), Usage::default());
    }

    #[test]
    fn reserve_failure_does_not_consume_waiver() {
        let meter = over_budget_meter();
        meter.approve_overrun(BudgetScope::Agent("Lumen".into()), "tester");
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, f64::NAN))
            .is_err());
        assert_eq!(
            meter
                .waiver_approver(&BudgetScope::Agent("Lumen".into()))
                .as_deref(),
            Some("tester")
        );
        let reservation = meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .expect("the waiver survives the failed reserve");
        reservation.settle(Usage::new(0, 10, 0.0));
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .is_err());
    }

    #[test]
    fn exceeded_check_is_read_only() {
        let meter = over_budget_meter();
        meter.approve_overrun(BudgetScope::Agent("Lumen".into()), "tester");
        assert_eq!(meter.exceeded("job-1", "Lumen"), None);
        assert_eq!(meter.exceeded("job-1", "Lumen"), None);
        assert_eq!(meter.exceeded("job-1", "Lumen"), None);
        assert!(meter
            .reserve("job-1", "Lumen", Usage::new(0, 10, 0.0))
            .is_ok());
    }

    #[test]
    fn approve_overrun_records_approver() {
        let meter = Arc::new(UsageMeter::new());
        meter.set_job_budget("job-1", Budget::tokens(100));
        meter.record("job-1", "Lumen", Usage::new(0, 150, 0.0));
        meter.register_node("polish", "Lumen");
        let approver = BudgetApprover::with_approver(
            meter.clone(),
            ScriptedApprover::new([ApprovalDecision::Approved]),
            "alice",
        );
        let request = ApprovalRequest {
            job: "job-1".into(),
            node: "polish".into(),
            prompt: "Approve?".into(),
        };
        let decision = run_blocking(approver.decide(&request));
        assert_eq!(decision, ApprovalDecision::Approved);
        assert_eq!(
            meter
                .waiver_approver(&BudgetScope::Job("job-1".into()))
                .as_deref(),
            Some("alice")
        );
        // The default constructor records an explicit anonymous identity.
        let meter2 = Arc::new(UsageMeter::new());
        meter2.set_job_budget("job-1", Budget::tokens(100));
        meter2.record("job-1", "Lumen", Usage::new(0, 150, 0.0));
        meter2.register_node("polish", "Lumen");
        let anon = BudgetApprover::new(
            meter2.clone(),
            ScriptedApprover::new([ApprovalDecision::Approved]),
        );
        let decision = run_blocking(anon.decide(&request));
        assert_eq!(decision, ApprovalDecision::Approved);
        assert_eq!(
            meter2
                .waiver_approver(&BudgetScope::Job("job-1".into()))
                .as_deref(),
            Some("anonymous")
        );
    }

    #[test]
    fn settle_with_invalid_actual_keeps_estimate() {
        let meter = UsageMeter::new();
        let reservation = meter
            .reserve("job-1", "Lumen", Usage::new(40, 20, 1.0))
            .unwrap();
        reservation.settle(Usage::new(0, 0, f64::NAN));
        let used = meter.job_usage("job-1");
        assert_eq!(used.total_tokens(), 60);
        assert!((used.cost - 1.0).abs() < 1e-9);
    }
}
