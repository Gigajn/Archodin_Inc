//! Nakama, the Agent that acts as HR.
//!
//! [`Nakama`] reads a [`Workload`] and the [`Roster`] and makes [`Proposal`]s:
//! hire an Agent from a Role, let an idle Agent go, or add a drafted new Role.
//! Proposing changes nothing; [`Nakama::resolve`] asks a person through an
//! [`Approver`] and applies only the proposals that are approved.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::{ApprovalDecision, ApprovalRequest, Approver, Role, Roster};

/// What Nakama watches: backlog per Role, idle Agents and work no Role fits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workload {
    backlog: Vec<(String, usize)>,
    idle: Vec<String>,
    unmatched: Vec<String>,
}

impl Workload {
    /// An empty workload.
    pub fn new() -> Self {
        Self::default()
    }

    /// Notes that `jobs` Jobs are waiting for the Role called `role`.
    pub fn backlog(mut self, role: impl Into<String>, jobs: usize) -> Self {
        self.backlog.push((role.into(), jobs));
        self
    }

    /// Notes that the Agent called `agent` has had nothing to do.
    pub fn idle(mut self, agent: impl Into<String>) -> Self {
        self.idle.push(agent.into());
        self
    }

    /// Notes work, described in words, that no existing Role fits.
    pub fn unmatched(mut self, work: impl Into<String>) -> Self {
        self.unmatched.push(work.into());
        self
    }
}

/// A change Nakama wants a person to approve.
#[derive(Clone)]
pub enum Proposal {
    /// Hire a new Agent called `agent` from the Role called `role`.
    Hire {
        /// The Role to hire from.
        role: String,
        /// The new Agent's name.
        agent: String,
    },
    /// Let the Agent called `agent`, of Role `role`, go.
    Release {
        /// The Agent to let go.
        agent: String,
        /// Its Role.
        role: String,
    },
    /// Add a drafted Role.
    NewRole {
        /// The drafted Role: instructions, Tools, Skills and Model chain.
        role: Role,
    },
}

impl PartialEq for Proposal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Hire {
                    role: r1,
                    agent: a1,
                },
                Self::Hire {
                    role: r2,
                    agent: a2,
                },
            ) => r1 == r2 && a1 == a2,
            (
                Self::Release {
                    agent: a1,
                    role: r1,
                },
                Self::Release {
                    agent: a2,
                    role: r2,
                },
            ) => a1 == a2 && r1 == r2,
            (Self::NewRole { role: a }, Self::NewRole { role: b }) => {
                a.name() == b.name() && a.instructions() == b.instructions()
            }
            _ => false,
        }
    }
}

impl std::fmt::Debug for Proposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hire { role, agent } => write!(f, "Hire {{ role: {role:?}, agent: {agent:?} }}"),
            Self::Release { agent, role } => {
                write!(f, "Release {{ agent: {agent:?}, role: {role:?} }}")
            }
            Self::NewRole { role } => write!(f, "NewRole {{ role: {:?} }}", role.name()),
        }
    }
}

impl Proposal {
    /// What the person is asked to approve.
    pub fn describe(&self) -> String {
        match self {
            Self::Hire { role, agent } => format!("Hire {agent} as {role}"),
            Self::Release { agent, role } => format!("Let {agent} ({role}) go"),
            Self::NewRole { role } => format!(
                "Create the Role {} with instructions: {}",
                role.name(),
                role.instructions()
            ),
        }
    }

    fn node(&self) -> &'static str {
        match self {
            Self::Hire { .. } => "hire",
            Self::Release { .. } => "release",
            Self::NewRole { .. } => "new-role",
        }
    }
}

type Drafter = Arc<dyn Fn(&str) -> Role + Send + Sync>;

/// The Agent that watches workload and proposes hires, departures and new Roles.
pub struct Nakama {
    names: Vec<String>,
    drafter: Drafter,
    threshold: usize,
    reserved: Mutex<HashSet<String>>,
}

impl Nakama {
    /// Nakama names new Agents from `names`, in order, and drafts a Role for
    /// unmatched work with `drafter`.
    pub fn new<N, F>(names: impl IntoIterator<Item = N>, drafter: F) -> Self
    where
        N: Into<String>,
        F: Fn(&str) -> Role + Send + Sync + 'static,
    {
        Self {
            names: names.into_iter().map(Into::into).collect(),
            drafter: Arc::new(drafter),
            threshold: 5,
            reserved: Mutex::new(HashSet::new()),
        }
    }

    /// Sets the backlog at which a Role gets a hiring proposal (default 5).
    pub fn backlog_threshold(mut self, jobs: usize) -> Self {
        self.threshold = jobs;
        self
    }

    /// Reads the workload and returns what Nakama proposes. Nothing changes.
    pub fn propose(&self, roster: &Roster, workload: &Workload) -> Vec<Proposal> {
        let mut reserved = self.reserved.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for (role, jobs) in &workload.backlog {
            if *jobs < self.threshold || roster.role(role).is_none() {
                continue;
            }
            let free = self
                .names
                .iter()
                .find(|n| roster.agent(n).is_none() && !reserved.contains(*n));
            if let Some(name) = free {
                reserved.insert(name.clone());
                out.push(Proposal::Hire {
                    role: role.clone(),
                    agent: name.clone(),
                });
            }
        }
        for name in &workload.idle {
            if let Some(agent) = roster.agent(name) {
                out.push(Proposal::Release {
                    agent: name.clone(),
                    role: agent.role().to_string(),
                });
            }
        }
        for work in &workload.unmatched {
            let role = (self.drafter)(work);
            if roster.role(role.name()).is_none() {
                out.push(Proposal::NewRole { role });
            }
        }
        out
    }

    /// Asks `approver` about each proposal in turn and applies the approved
    /// ones to `roster`. Rejected and pending proposals change nothing. Returns
    /// the proposals that took effect.
    pub async fn resolve(
        &self,
        roster: &Roster,
        proposals: Vec<Proposal>,
        approver: &dyn Approver,
    ) -> Vec<Proposal> {
        let mut applied = Vec::new();
        for proposal in proposals {
            let request = ApprovalRequest {
                job: "nakama".into(),
                node: proposal.node().into(),
                prompt: proposal.describe(),
            };
            let decision = approver.decide(&request).await;
            if let Proposal::Hire { agent, .. } = &proposal {
                if decision != ApprovalDecision::Pending {
                    self.reserved
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(agent);
                }
            }
            if decision != ApprovalDecision::Approved {
                continue;
            }
            let done = match &proposal {
                Proposal::Hire { role, agent } => roster.hire(role, agent).is_ok(),
                Proposal::Release { agent, .. } => roster.release(agent).is_some(),
                Proposal::NewRole { role } => {
                    roster.add_role(role.clone());
                    true
                }
            };
            if done {
                applied.push(proposal);
            }
        }
        applied
    }
}
