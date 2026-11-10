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
//! # Semantics (intentional, KR-05)
//!
//! * An approved overrun is a **single-use, time-limited waiver**: one approval
//!   lets exactly one over-budget step through, for at most
//!   [`DEFAULT_WAIVER_TTL`]. The next overspend asks again (story 36).
//! * [`UsageMeter::reserve`] is the atomic gate: in one lock it checks the
//!   **projected** usage (`used + estimated`) against the Budget, consumes the
//!   waiver if the projection only passes because of it, and charges the
//!   estimate. A failed reserve changes nothing — no waiver is consumed and
//!   nothing is charged.
//! * [`Reservation`] is an RAII guard: [`Reservation::settle`] corrects the
//!   charged estimate to the actual usage (refund or top-up); dropping it
//!   without settling refunds the estimate. There is no manual `cancel()`.
//! * The meter is fail-closed on bad numbers: [`Pricing::per_million`],
//!   [`Budget::cost`] and [`Budget::with_cost`] reject NaN, infinite and
//!   negative values; [`UsageMeter::record`] ignores invalid usage instead of
//!   letting a NaN poison a scope's total forever; `exceeded_by` treats an
//!   unknown (NaN) cost as exceeded. `Pricing::price()` can still yield `+inf`
//!   by `f64` overflow, but that fails safe (`inf` exceeds any Budget).
//! * A metered node whose reservation fails does **not** run and records
//!   nothing; the Job pauses at the next metered node and approval is asked
//!   again. (The node closure cannot return an error, so the pause happens at
//!   the next metered node; if the failed node is the last metered node, the
//!   Job may finish without that node's effect — visible in the State.)
//! * A metered node has no cost estimate before it runs, so it reserves `0`:
//!   the reserve is an atomic gate and the waiver-consumption point, not a
//!   bound on parallel overspend. Callers that know an estimate should use
//!   `reserve()` directly; that is where concurrent reserves are bounded.
//!
//! The meter lives in memory: usage recorded before a process restart is not
//! restored when a Job resumes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;

use crate::{ApprovalDecision, ApprovalRequest, Approver, GraphBuilder};

/// How long one approved overrun stays usable: a single over-budget step may
/// go through within this long of the approval, then the Job asks again.
pub const DEFAULT_WAIVER_TTL: Duration = Duration::from_secs(3_600);

/// Why a Budget operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BudgetError {
    /// A price per million tokens was NaN, infinite or negative.
    #[error("invalid price: {0}")]
    InvalidPrice(&'static str),
    /// A money Budget was NaN, infinite or negative.
    #[error("invalid budget: {0}")]
    InvalidBudget(&'static str),
    /// A usage (usually an estimate or an actual) was NaN, infinite or negative.
    #[error("invalid usage: {0}")]
    InvalidUsage(&'static str),
    /// The projected usage would exceed this Budget and no waiver covers it.
    #[error("budget exceeded for {0:?}")]
    Exceeded(BudgetScope),
}

/// A cost is usable only when it is finite and non-negative.
fn is_valid_usage(usage: &Usage) -> bool {
    usage.cost.is_finite() && usage.cost >= 0.0
}

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
    ///
    /// This constructor does NOT validate: the fields are `pub`, so a struct
    /// literal can hold any cost, including NaN. Validation happens at the
    /// meter boundary — [`UsageMeter::record`], [`UsageMeter::reserve`] and
    /// [`Reservation::settle`] all reject invalid usage.
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
    ///
    /// Fails when either price is NaN, infinite or negative.
    pub fn per_million(prompt: f64, completion: f64) -> Result<Self, BudgetError> {
        if !prompt.is_finite() || prompt < 0.0 {
            return Err(BudgetError::InvalidPrice(
                "prompt price per million must be finite and non-negative",
            ));
        }
        if !completion.is_finite() || completion < 0.0 {
            return Err(BudgetError::InvalidPrice(
                "completion price per million must be finite and non-negative",
            ));
        }
        Ok(Self {
            prompt_per_million: prompt,
            completion_per_million: completion,
        })
    }

    /// `usage` with its cost set from its tokens.
    ///
    /// Can still yield `+inf` by `f64` overflow from very large token counts;
    /// that fails safe, because `inf` exceeds any Budget.
    pub fn price(&self, usage: Usage) -> Usage {
        Usage {
            cost: usage.prompt_tokens as f64 * self.prompt_per_million / 1_000_000.0
                + usage.completion_tokens as f64 * self.completion_per_million / 1_000_000.0,
            ..usage
        }
    }
}

/// The most tokens or money an Agent or a Job may spend before it asks a person.
///
/// `Budget::default()` means **no limit**: a scope with the default Budget is
/// never asked about. That is the deliberate sentinel for "no cap set" — the
/// capped default for Agents lives at the [`Role`](crate::Role) level, not here.
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
    ///
    /// Fails when `max` is NaN, infinite or negative.
    pub fn cost(max: f64) -> Result<Self, BudgetError> {
        if !max.is_finite() || max < 0.0 {
            return Err(BudgetError::InvalidBudget(
                "max cost must be finite and non-negative",
            ));
        }
        Ok(Self {
            max_tokens: None,
            max_cost: Some(max),
        })
    }

    /// Also limits tokens.
    pub fn with_tokens(mut self, max: u64) -> Self {
        self.max_tokens = Some(max);
        self
    }

    /// Also limits money.
    ///
    /// Fails when `max` is NaN, infinite or negative.
    pub fn with_cost(mut self, max: f64) -> Result<Self, BudgetError> {
        if !max.is_finite() || max < 0.0 {
            return Err(BudgetError::InvalidBudget(
                "max cost must be finite and non-negative",
            ));
        }
        self.max_cost = Some(max);
        Ok(self)
    }

    /// Fail-closed: an unknown (NaN) cost, on either side, counts as exceeded.
    fn exceeded_by(&self, used: &Usage) -> bool {
        self.max_tokens.is_some_and(|m| used.total_tokens() > m)
            || self
                .max_cost
                .is_some_and(|m| used.cost.is_nan() || m.is_nan() || used.cost > m)
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

/// One approved overrun: who approved it, when, and until when it lets a
/// single over-budget reservation through. Single-use and time-limited; a
/// waiver grants no second overspend and does not survive its expiry.
/// (`Instant` cannot be persisted — if the meter is ever persisted, this must
/// move to `SystemTime`.)
#[derive(Debug)]
struct Waiver {
    approver: String,
    granted_at: Instant,
    expires_at: Instant,
}

fn is_waiver_valid(waivers: &HashMap<BudgetScope, Waiver>, scope: &BudgetScope) -> bool {
    waivers
        .get(scope)
        .is_some_and(|w| Instant::now() < w.expires_at)
}

#[derive(Default)]
struct MeterState {
    by_job: HashMap<String, Usage>,
    by_agent: HashMap<String, Usage>,
    job_budgets: HashMap<String, Budget>,
    agent_budgets: HashMap<String, Budget>,
    waivers: HashMap<BudgetScope, Waiver>,
    node_agents: HashMap<String, String>,
}

/// Lock a mutex, recovering from poisoning like the helper in `roster.rs:111`.
/// New code in this file must use this instead of `lock().unwrap()` (KR-11).
/// NOTE: this is a temporary duplicate of `roster.rs:111`. The KR-06+KR-11
/// group provides the shared `archodin_graph::locks::mutex` on their branch;
/// when the branches merge, both helpers are replaced by that one. Do not
/// create a second shared helper here.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Adds two usages, saturating the token counts instead of overflowing.
fn saturating_add_usage(a: Usage, b: Usage) -> Usage {
    Usage {
        prompt_tokens: a.prompt_tokens.saturating_add(b.prompt_tokens),
        completion_tokens: a.completion_tokens.saturating_add(b.completion_tokens),
        cost: a.cost + b.cost,
    }
}

/// Corrects a recorded usage by `actual - estimated`, clamped at zero.
fn adjust_usage(map: &mut HashMap<String, Usage>, key: &str, estimated: Usage, actual: Usage) {
    let used = map.entry(key.to_string()).or_default();
    used.prompt_tokens = (used.prompt_tokens as i128 + actual.prompt_tokens as i128
        - estimated.prompt_tokens as i128)
        .clamp(0, u64::MAX as i128) as u64;
    used.completion_tokens = (used.completion_tokens as i128 + actual.completion_tokens as i128
        - estimated.completion_tokens as i128)
        .clamp(0, u64::MAX as i128) as u64;
    used.cost = (used.cost + actual.cost - estimated.cost).max(0.0);
}

/// Removes a charged estimate again, clamped at zero.
fn refund_usage(map: &mut HashMap<String, Usage>, key: &str, estimated: Usage) {
    if let Some(used) = map.get_mut(key) {
        used.prompt_tokens = used.prompt_tokens.saturating_sub(estimated.prompt_tokens);
        used.completion_tokens = used
            .completion_tokens
            .saturating_sub(estimated.completion_tokens);
        used.cost = (used.cost - estimated.cost).max(0.0);
    }
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
    ///
    /// Invalid usage (NaN, infinite or negative cost) is ignored — it is
    /// never added to a total, so one bad report cannot poison a scope's
    /// total into NaN forever. Prefer [`UsageMeter::reserve`] plus
    /// [`Reservation::settle`] for new code; this is the legacy non-atomic path.
    pub fn record(&self, job: &str, agent: &str, usage: Usage) {
        if !is_valid_usage(&usage) {
            eprintln!(
                "KR-05: ignoring invalid usage for job `{job}` agent `{agent}`: cost {:?} is not finite and non-negative",
                usage.cost
            );
            return;
        }
        let mut s = lock(&self.state);
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
    ///
    /// Resetting a Budget voids any outstanding waiver for that scope: an
    /// approval granted under the old Budget does not carry over.
    pub fn set_job_budget(&self, job: &str, budget: Budget) {
        let mut s = self.state.lock().unwrap();
        s.job_budgets.insert(job.to_string(), budget);
        let _ = s.waivers.remove(&BudgetScope::Job(job.to_string()));
    }

    /// Sets the Budget of an Agent, counted across all its Jobs.
    ///
    /// Resetting a Budget voids any outstanding waiver for that scope.
    pub fn set_agent_budget(&self, agent: &str, budget: Budget) {
        let mut s = self.state.lock().unwrap();
        s.agent_budgets.insert(agent.to_string(), budget);
        let _ = s.waivers.remove(&BudgetScope::Agent(agent.to_string()));
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

    /// Read-only check: reports the first exceeded Budget and why, without
    /// consuming anything. Callers that only build a prompt (or decide whether
    /// to ask at all) must use this — evaluating a prompt must never burn the
    /// waiver for the overspend it describes.
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
            (budget.exceeded_by(&used) && !is_waiver_valid(&s.waivers, &scope)).then(|| {
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

    /// Records that `approver` approved the overrun of this Budget, usable for
    /// one over-budget step within [`DEFAULT_WAIVER_TTL`]. The waiver is
    /// consumed when that step reserves (see [`UsageMeter::reserve`]); a second
    /// overspend asks again.
    pub fn approve_overrun(&self, scope: BudgetScope, approver: impl Into<String>) {
        self.approve_overrun_with_ttl(scope, approver, DEFAULT_WAIVER_TTL);
    }

    /// Like [`UsageMeter::approve_overrun`] with an explicit lifetime for the
    /// waiver. This is the seam tests use to build an already-expired waiver
    /// (`Duration::ZERO`) without sleeping.
    pub fn approve_overrun_with_ttl(
        &self,
        scope: BudgetScope,
        approver: impl Into<String>,
        ttl: Duration,
    ) {
        let now = Instant::now();
        let waiver = Waiver {
            approver: approver.into(),
            granted_at: now,
            expires_at: now + ttl,
        };
        let _ = lock(&self.state).waivers.insert(scope, waiver);
    }

    /// Consumes the valid waiver for `scope`, if there is one. Returns whether
    /// a waiver was consumed. This is the single consumption point for direct
    /// callers. [`UsageMeter::reserve`] performs the same consumption
    /// atomically under its own lock (it cannot call this method — that would
    /// re-lock); the read-only [`UsageMeter::exceeded`] never consumes.
    pub fn try_consume_waiver(&self, scope: &BudgetScope) -> bool {
        let mut s = lock(&self.state);
        if is_waiver_valid(&s.waivers, scope) {
            let _ = s.waivers.remove(scope);
            true
        } else {
            false
        }
    }

    /// Who approved the currently valid waiver for `scope`, if there is one.
    /// `None` when there is no waiver or it expired / was consumed.
    pub fn waiver_approver(&self, scope: &BudgetScope) -> Option<String> {
        let s = lock(&self.state);
        if is_waiver_valid(&s.waivers, scope) {
            s.waivers.get(scope).map(|w| w.approver.clone())
        } else {
            None
        }
    }

    /// Atomically reserves `estimated` usage for `agent` working on `job`:
    /// in one lock, checks the projected usage (`used + estimated`) of both
    /// the Job and the Agent Budget, consumes the waiver of any scope whose
    /// projection only passes because of it, and charges the estimate to both
    /// totals. Returns the [`Reservation`] that must be settled (or dropped)
    /// once the step is done.
    ///
    /// Fails with [`BudgetError::Exceeded`] when a projection would exceed a
    /// Budget with no valid waiver, and with [`BudgetError::InvalidUsage`]
    /// when the estimate itself is invalid. A failed reserve changes nothing:
    /// no waiver is consumed and nothing is charged.
    ///
    /// A single estimate larger than a whole Budget can never reserve without
    /// an approval — that is the intended consequence of the projection check.
    pub fn reserve(
        &self,
        job: &str,
        agent: &str,
        estimated: Usage,
    ) -> Result<Reservation<'_>, BudgetError> {
        if !is_valid_usage(&estimated) {
            return Err(BudgetError::InvalidUsage(
                "estimated cost must be finite and non-negative",
            ));
        }
        let mut s = lock(&self.state);
        let checks = [
            (
                BudgetScope::Job(job.to_string()),
                s.job_budgets.get(job).copied(),
                s.by_job.get(job).copied().unwrap_or_default(),
            ),
            (
                BudgetScope::Agent(agent.to_string()),
                s.agent_budgets.get(agent).copied(),
                s.by_agent.get(agent).copied().unwrap_or_default(),
            ),
        ];
        // Decide first, change nothing until every scope passes.
        let mut consume: Vec<BudgetScope> = Vec::new();
        for (scope, budget, used) in &checks {
            if let Some(budget) = budget {
                let projected = saturating_add_usage(*used, estimated);
                if budget.exceeded_by(&projected) {
                    if is_waiver_valid(&s.waivers, scope) {
                        consume.push(scope.clone());
                    } else {
                        return Err(BudgetError::Exceeded(scope.clone()));
                    }
                }
            }
        }
        for scope in &consume {
            let _ = s.waivers.remove(scope);
        }
        s.by_job.entry(job.to_string()).or_default().add(estimated);
        s.by_agent
            .entry(agent.to_string())
            .or_default()
            .add(estimated);
        Ok(Reservation {
            meter: self,
            job: job.to_string(),
            agent: agent.to_string(),
            estimated,
            settled: false,
        })
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

/// A reservation made by [`UsageMeter::reserve`]: the estimate is already
/// charged to the Job and the Agent. Settle it once the step is done, or drop
/// it to refund the estimate. Dropping without settling is the cancel path —
/// there is no manual `cancel()` method.
#[must_use = "a Reservation refunds its estimate when dropped without settle()"]
pub struct Reservation<'a> {
    meter: &'a UsageMeter,
    job: String,
    agent: String,
    estimated: Usage,
    settled: bool,
}

impl Reservation<'_> {
    /// Corrects the charged estimate to the actual usage: a refund when the
    /// step used less than estimated, a top-up when it used more.
    ///
    /// An invalid actual (NaN, infinite or negative cost) keeps the estimated
    /// charge — fail-safe towards over-charging — instead of recording a
    /// number that would poison the totals.
    pub fn settle(mut self, actual: Usage) {
        self.settled = true;
        if !is_valid_usage(&actual) {
            eprintln!(
                "KR-05: settle called with invalid usage for job `{}` agent `{}`: cost {:?} is not finite and non-negative; keeping the estimated charge",
                self.job, self.agent, actual.cost
            );
            return;
        }
        let mut s = lock(&self.meter.state);
        adjust_usage(&mut s.by_job, &self.job, self.estimated, actual);
        adjust_usage(&mut s.by_agent, &self.agent, self.estimated, actual);
    }
}

impl Drop for Reservation<'_> {
    /// Refunds the estimate unless the reservation was settled. This is what
    /// closes the gap for a cancelled or panicking node future: nobody has to
    /// remember to call `cancel()`. Only a truly dead process skips the
    /// refund, and then the estimate stays charged (fail-safe over-charge).
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let mut s = lock(&self.meter.state);
        refund_usage(&mut s.by_job, &self.job, self.estimated);
        refund_usage(&mut s.by_agent, &self.agent, self.estimated);
    }
}

/// An [`Approver`] that answers for metered steps within their Budgets and hands
/// the questions about exceeded Budgets to the real approval interface.
pub struct BudgetApprover<A> {
    meter: Arc<UsageMeter>,
    inner: A,
    /// Who approves overruns through this wrapper, recorded on the waiver.
    /// `"anonymous"` when the caller did not supply an identity.
    approver: String,
}

impl<A: Approver> BudgetApprover<A> {
    /// Wraps `inner`, which a person answers through. Approvals granted
    /// through this wrapper are recorded as `"anonymous"` — prefer
    /// [`BudgetApprover::with_approver`] when the approver's identity is known.
    pub fn new(meter: Arc<UsageMeter>, inner: A) -> Self {
        Self {
            meter,
            inner,
            approver: "anonymous".to_string(),
        }
    }

    /// Wraps `inner`, recording `approver` (for example from the Agora session
    /// that built this wrapper) on every waiver this approver grants.
    pub fn with_approver(meter: Arc<UsageMeter>, inner: A, approver: impl Into<String>) -> Self {
        Self {
            meter,
            inner,
            approver: approver.into(),
        }
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
                self.meter.approve_overrun(scope, self.approver.clone());
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
                // Reserve before running. The node has no cost estimate before
                // `f` runs, so it reserves 0: an atomic gate that also
                // consumes the single-use waiver when the scope is already
                // over budget. If the reservation fails — a racing node
                // consumed the waiver after the approval — the node does not
                // run and records nothing; the Job pauses at the next metered
                // node and approval is asked again.
                let Ok(reservation) = meter.reserve(&job, &agent, Usage::default()) else {
                    return s;
                };
                let (updated, usage) = f(s).await;
                reservation.settle(usage);
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
    use crate::ScriptedApprover;
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
