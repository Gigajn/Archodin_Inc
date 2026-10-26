//! Agent performance reviews.
//!
//! A [`PerformanceLedger`] keeps one record per Agent, updated after every Job:
//! Jobs finished, corrections from Kudo, Qadi decisions won or lost, cost and
//! Budget overruns. [`PerformanceLedger::report`] returns a read-only
//! [`PerformanceReport`] that the Agora can render next to each Agent's name and
//! Role, and that Nakama cites in its proposals ([`Nakama::propose_cited`]).

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::{Approver, Nakama, Proposal, Roster, Usage, Workload};

/// One Agent's performance figures.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentFigures {
    /// The Agent's name.
    pub agent: String,
    /// Jobs the Agent finished.
    pub jobs_finished: u64,
    /// Times Kudo corrected the Agent's work.
    pub corrections: u64,
    /// Qadi decisions the Agent's value won.
    pub decisions_won: u64,
    /// Qadi decisions the Agent's value lost.
    pub decisions_lost: u64,
    /// Times the Agent went over a Budget.
    pub budget_overruns: u64,
    /// Total cost of the Agent's finished Jobs.
    pub total_cost: f64,
}

impl AgentFigures {
    /// Average cost per finished Job, or `None` before the first Job.
    pub fn cost_per_job(&self) -> Option<f64> {
        (self.jobs_finished > 0).then(|| self.total_cost / self.jobs_finished as f64)
    }

    /// The figures in one line, as cited in proposals and shown in the Agora.
    pub fn summary(&self) -> String {
        let cost = self
            .cost_per_job()
            .map_or("n/a".to_string(), |c| format!("{c:.4}"));
        format!(
            "{}: {} Jobs, {} corrections, {} decisions won, {} lost, cost per Job {}, {} Budget overruns",
            self.agent,
            self.jobs_finished,
            self.corrections,
            self.decisions_won,
            self.decisions_lost,
            cost,
            self.budget_overruns
        )
    }
}

/// A read-only snapshot of every Agent's figures, sorted by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerformanceReport {
    agents: Vec<AgentFigures>,
}

impl PerformanceReport {
    /// All records, sorted by Agent name.
    pub fn agents(&self) -> &[AgentFigures] {
        &self.agents
    }

    /// The record of the Agent called `name`, if it has one.
    pub fn agent(&self, name: &str) -> Option<&AgentFigures> {
        self.agents.iter().find(|a| a.agent == name)
    }
}

/// Keeps each Agent's performance record up to date.
#[derive(Default)]
pub struct PerformanceLedger {
    records: Mutex<BTreeMap<String, AgentFigures>>,
}

impl PerformanceLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    fn update(&self, agent: &str, f: impl FnOnce(&mut AgentFigures)) {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let record = records
            .entry(agent.to_string())
            .or_insert_with(|| AgentFigures {
                agent: agent.to_string(),
                ..Default::default()
            });
        f(record);
    }

    /// Records that `agent` finished a Job, having used `usage`.
    pub fn job_finished(&self, _job: &str, agent: &str, usage: Usage) {
        self.update(agent, |r| {
            r.jobs_finished += 1;
            r.total_cost += usage.cost;
        });
    }

    /// Records that Kudo corrected `agent`'s work.
    pub fn correction(&self, agent: &str) {
        self.update(agent, |r| r.corrections += 1);
    }

    /// Records that `agent` went over a Budget.
    pub fn budget_overrun(&self, agent: &str) {
        self.update(agent, |r| r.budget_overruns += 1);
    }

    /// Records a Qadi decision: `winner`'s value was chosen over each of `losers`.
    pub fn qadi_decision<'a>(&self, winner: &str, losers: impl IntoIterator<Item = &'a str>) {
        self.update(winner, |r| r.decisions_won += 1);
        for loser in losers {
            self.update(loser, |r| r.decisions_lost += 1);
        }
    }

    /// A read-only snapshot of every record.
    pub fn report(&self) -> PerformanceReport {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        PerformanceReport {
            agents: records.values().cloned().collect(),
        }
    }
}

/// A [`Proposal`] together with the performance figures that led to it.
#[derive(Debug, Clone, PartialEq)]
pub struct CitedProposal {
    /// What Nakama proposes.
    pub proposal: Proposal,
    /// The figures behind it: the Agents of the Role for a hire, the Agent for a departure.
    pub figures: Vec<AgentFigures>,
}

impl CitedProposal {
    /// What the person is asked to approve, with the figures cited.
    pub fn describe(&self) -> String {
        let mut text = self.proposal.describe();
        if !self.figures.is_empty() {
            let lines: Vec<_> = self.figures.iter().map(AgentFigures::summary).collect();
            text.push_str(&format!("\nPerformance: {}", lines.join("; ")));
        }
        text
    }
}

impl Nakama {
    /// Like [`Nakama::propose`], citing the figures from `report` behind each proposal.
    pub fn propose_cited(
        &self,
        roster: &Roster,
        workload: &Workload,
        report: &PerformanceReport,
    ) -> Vec<CitedProposal> {
        self.propose(roster, workload)
            .into_iter()
            .map(|proposal| {
                let names: Vec<String> = match &proposal {
                    Proposal::Hire { role, .. } => roster
                        .agents()
                        .iter()
                        .filter(|a| a.role() == role)
                        .map(|a| a.name().to_string())
                        .collect(),
                    Proposal::Release { agent, .. } => vec![agent.clone()],
                    Proposal::NewRole { .. } => Vec::new(),
                };
                let figures = names
                    .iter()
                    .filter_map(|n| report.agent(n).cloned())
                    .collect();
                CitedProposal { proposal, figures }
            })
            .collect()
    }

    /// Like [`Nakama::resolve`], with the cited figures in each approval prompt.
    pub async fn resolve_cited(
        &self,
        roster: &Roster,
        proposals: Vec<CitedProposal>,
        approver: &dyn Approver,
    ) -> Vec<Proposal> {
        let prompted = proposals
            .into_iter()
            .map(|c| (c.describe(), c.proposal))
            .collect();
        self.resolve_prompted(roster, prompted, approver).await
    }
}
