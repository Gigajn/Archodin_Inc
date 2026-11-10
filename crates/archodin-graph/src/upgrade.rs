//! Versioned Departments and Explorador's upgrade Charters.
//!
//! A [`VersionedDepartment`] keeps the setup of a running [`Department`] (Roles
//! and Agent counts, Skills, Graph, Budget) as numbered versions. Explorador
//! proposes an [`UpgradeCharter`] with [`Explorador::propose_upgrade`], which
//! lists what changes and changes nothing itself. Only when a person approves it
//! ([`Explorador::approve_upgrade`]) does it become the next version: Nakama
//! hires the extra Agents and the Department's Budget on the [`UsageMeter`] is
//! updated. A [`RunningJob`] is pinned to the version current when it started, so
//! upgrades and rollbacks only affect Jobs started afterwards.
//! [`VersionedDepartment::rollback`] makes the previous version current again;
//! every version stays in the history and version numbers are never reused.
//! Agents hired for an upgrade stay hired after a rollback; letting them go is a
//! Nakama proposal like any other.

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use crate::{
    ApprovalDecision, ApprovalRequest, Approver, Budget, Department, Explorador, Nakama, Proposal,
    Roster, UsageMeter, Workload,
};
use crate::locks::mutex;

/// One version of a Department's setup.
#[derive(Debug, Clone, PartialEq)]
pub struct DepartmentSetup {
    roles: Vec<(String, usize)>,
    skills: Vec<String>,
    graph: Option<String>,
    budget: Budget,
}

impl DepartmentSetup {
    /// Its Roles and how many Agents each has.
    pub fn roles(&self) -> &[(String, usize)] {
        &self.roles
    }

    /// The Skills added by upgrades.
    pub fn skills(&self) -> &[String] {
        &self.skills
    }

    /// The name of the Graph its Jobs follow, once an upgrade has set one.
    pub fn graph(&self) -> Option<&str> {
        self.graph.as_deref()
    }

    /// Its Budget.
    pub fn budget(&self) -> Budget {
        self.budget
    }
}

struct Entry {
    version: u32,
    setup: DepartmentSetup,
}

struct Versions {
    all: Vec<Entry>,
    /// Version numbers in effect, oldest first; the last one is current.
    active: Vec<u32>,
}

impl Versions {
    fn entry(&self, version: u32) -> &Entry {
        self.all
            .iter()
            .find(|e| e.version == version)
            .expect("active versions exist")
    }

    fn current(&self) -> &Entry {
        self.entry(*self.active.last().expect("version 1 always exists"))
    }
}

/// A [`Department`] with its setup kept as versions. Clones share state.
#[derive(Clone)]
pub struct VersionedDepartment {
    department: Department,
    versions: Arc<Mutex<Versions>>,
}

impl VersionedDepartment {
    /// Starts the history of `department` with version 1, its original Charter.
    pub fn new(department: Department) -> Self {
        let charter = department.charter();
        let setup = DepartmentSetup {
            roles: charter.roles().to_vec(),
            skills: Vec::new(),
            graph: None,
            budget: charter.budget(),
        };
        Self {
            department,
            versions: Arc::new(Mutex::new(Versions {
                all: vec![Entry { version: 1, setup }],
                active: vec![1],
            })),
        }
    }

    /// The Department itself.
    pub fn department(&self) -> &Department {
        &self.department
    }

    /// The version new Jobs use.
    pub fn version(&self) -> u32 {
        mutex(&self.versions).current().version
    }

    /// The setup new Jobs use.
    pub fn setup(&self) -> DepartmentSetup {
        mutex(&self.versions).current().setup.clone()
    }

    /// Every version ever made, rolled-back ones included, oldest first.
    pub fn history(&self) -> Vec<(u32, DepartmentSetup)> {
        mutex(&self.versions)
            .all
            .iter()
            .map(|e| (e.version, e.setup.clone()))
            .collect()
    }

    /// Starts a Job on the current version. The Job keeps that version even if
    /// the Department is upgraded or rolled back while it runs.
    pub fn start_job(&self) -> RunningJob {
        let versions = mutex(&self.versions);
        let current = versions.current();
        RunningJob {
            version: current.version,
            setup: current.setup.clone(),
        }
    }

    /// Makes the previous version current again for new Jobs, registering its
    /// Budget on `meter`, and returns it. Running Jobs are not affected.
    pub fn rollback(&self, meter: &UsageMeter) -> Result<u32, UpgradeError> {
        let mut versions = mutex(&self.versions);
        if versions.active.len() < 2 {
            return Err(UpgradeError::NothingToRollBack);
        }
        versions.active.pop();
        let current = versions.current();
        meter.set_agent_budget(&self.department.budget_key(), current.setup.budget);
        Ok(current.version)
    }
}

/// A Job's view of its Department: the version it started on.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningJob {
    version: u32,
    setup: DepartmentSetup,
}

impl RunningJob {
    /// The version the Job started on.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The setup of that version.
    pub fn setup(&self) -> &DepartmentSetup {
        &self.setup
    }
}

/// What an upgrade changes, built step by step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Upgrade {
    add_agents: Vec<(String, usize)>,
    skills: Vec<String>,
    graph: Option<String>,
    budget: Option<Budget>,
}

impl Upgrade {
    /// An upgrade that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `count` Agents of the Role called `role` (a new Role if the
    /// Department has none of it yet).
    pub fn add_agents(mut self, role: impl Into<String>, count: usize) -> Self {
        self.add_agents.push((role.into(), count));
        self
    }

    /// Adds a Skill.
    pub fn skill(mut self, skill: impl Into<String>) -> Self {
        self.skills.push(skill.into());
        self
    }

    /// Changes the Graph its Jobs follow.
    pub fn graph(mut self, name: impl Into<String>) -> Self {
        self.graph = Some(name.into());
        self
    }

    /// Changes the Budget.
    pub fn budget(mut self, budget: Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    fn apply(&self, from: &DepartmentSetup) -> DepartmentSetup {
        let mut next = from.clone();
        for (role, count) in &self.add_agents {
            match next.roles.iter_mut().find(|(r, _)| r == role) {
                Some((_, n)) => *n += count,
                None => next.roles.push((role.clone(), *count)),
            }
        }
        for skill in &self.skills {
            if !next.skills.contains(skill) {
                next.skills.push(skill.clone());
            }
        }
        if let Some(graph) = &self.graph {
            next.graph = Some(graph.clone());
        }
        if let Some(budget) = self.budget {
            next.budget = budget;
        }
        next
    }
}

/// A proposal to upgrade a running Department. Nothing changes until a person
/// approves it.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeCharter {
    department: String,
    from_version: u32,
    changes: Vec<String>,
    upgrade: Upgrade,
    setup: DepartmentSetup,
}

impl UpgradeCharter {
    /// The name of the Department it upgrades.
    pub fn department(&self) -> &str {
        &self.department
    }

    /// The version it was drafted against.
    pub fn from_version(&self) -> u32 {
        self.from_version
    }

    /// One line per change.
    pub fn changes(&self) -> &[String] {
        &self.changes
    }

    /// The setup the next version would have.
    pub fn setup(&self) -> &DepartmentSetup {
        &self.setup
    }

    /// What the person is asked to approve.
    pub fn describe(&self) -> String {
        format!(
            "Upgrade the Department {} from version {}: {}",
            self.department,
            self.from_version,
            self.changes.join("; ")
        )
    }
}

/// Why an upgrade operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpgradeError {
    /// The Department changed after the Charter was drafted.
    #[error(
        "the Charter was drafted for version {drafted} but the Department is on version {current}"
    )]
    Stale {
        /// The version the Charter was drafted against.
        drafted: u32,
        /// The version the Department is on now.
        current: u32,
    },
    /// The Department is still on its original setup.
    #[error("the Department has no earlier version to roll back to")]
    NothingToRollBack,
}

impl Explorador {
    /// Drafts an [`UpgradeCharter`] for a running Department showing what
    /// `upgrade` changes against its current version. Nothing is changed.
    pub fn propose_upgrade(
        &self,
        department: &VersionedDepartment,
        upgrade: Upgrade,
    ) -> UpgradeCharter {
        let (from_version, from) = {
            let versions = mutex(&department.versions);
            let current = versions.current();
            (current.version, current.setup.clone())
        };
        let setup = upgrade.apply(&from);
        let mut changes = Vec::new();
        for (role, count) in &upgrade.add_agents {
            let had = from
                .roles
                .iter()
                .find(|(r, _)| r == role)
                .map_or(0, |(_, n)| *n);
            changes.push(format!("{role}: {had} -> {} Agents", had + count));
        }
        for skill in upgrade.skills.iter().filter(|s| !from.skills.contains(s)) {
            changes.push(format!("add Skill {skill}"));
        }
        if let Some(graph) = upgrade.graph.as_ref().filter(|g| from.graph() != Some(g)) {
            changes.push(format!(
                "Graph: {} -> {graph}",
                from.graph().unwrap_or("none")
            ));
        }
        if let Some(budget) = upgrade.budget.filter(|b| *b != from.budget) {
            changes.push(format!("Budget: {:?} -> {budget:?}", from.budget));
        }
        UpgradeCharter {
            department: department.department.name().to_string(),
            from_version,
            changes,
            upgrade,
            setup,
        }
    }

    /// Asks `approver` about the upgrade. Only when it is approved does it become
    /// the Department's next version: `nakama` hires the extra Agents from
    /// `roster` and the new Budget is registered on `meter`. Returns the new
    /// version number, or `None` when the upgrade was rejected or is still
    /// pending. A Charter drafted for an older version is refused without asking.
    pub async fn approve_upgrade(
        &self,
        charter: UpgradeCharter,
        approver: &dyn Approver,
        department: &VersionedDepartment,
        roster: &Roster,
        nakama: &Nakama,
        meter: &UsageMeter,
    ) -> Result<Option<u32>, UpgradeError> {
        let current = department.version();
        if charter.from_version != current {
            return Err(UpgradeError::Stale {
                drafted: charter.from_version,
                current,
            });
        }
        let request = ApprovalRequest {
            job: "explorador".into(),
            node: "upgrade".into(),
            prompt: charter.describe(),
        };
        if approver.decide(&request).await != ApprovalDecision::Approved {
            return Ok(None);
        }
        // The person approved the upgrade, so the hires it names need no second ask.
        for (role, count) in &charter.upgrade.add_agents {
            for _ in 0..*count {
                let workload = Workload::new().backlog(role.clone(), usize::MAX);
                let proposals = nakama.propose(roster, &workload);
                for applied in nakama.resolve(roster, proposals, &AlwaysApproved).await {
                    if let Proposal::Hire { agent, .. } = applied {
                        department.department.add_agent(agent);
                    }
                }
            }
        }
        meter.set_agent_budget(&department.department.budget_key(), charter.setup.budget);
        let mut versions = mutex(&department.versions);
        let version = versions.all.last().map_or(1, |e| e.version) + 1;
        versions.all.push(Entry {
            version,
            setup: charter.setup,
        });
        versions.active.push(version);
        Ok(Some(version))
    }
}

struct AlwaysApproved;

impl Approver for AlwaysApproved {
    fn decide(&self, _: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision> {
        Box::pin(async { ApprovalDecision::Approved })
    }
}
