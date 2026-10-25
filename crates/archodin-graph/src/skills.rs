//! Role-owned Skills and Codex's approval queue (ADR 0004).
//!
//! A [`Skill`] is a `SKILL.md` (agentskills.io format) that is either a Flow or a
//! Discipline. A [`SkillLibrary`] plays Codex: every new or changed Skill waits as
//! pending until a person approves it through the [`Approver`] interface, approved
//! Skills belong to a Role (so every Agent of that Role has them), every change is
//! kept as a version that can be rolled back, and a Skill can be marked shared
//! across Roles. A Skill's scripts stay disabled until approved separately.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{Agent, ApprovalDecision, ApprovalRequest, Approver};

/// Whether a Skill is started on purpose or reached for by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    /// Started by a person or a Strategos; may use Disciplines, never another Flow.
    Flow,
    /// Reached for by an Agent whenever the task fits.
    Discipline,
}

/// Why a Skill operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    /// The `SKILL.md` text could not be read.
    #[error("invalid skill: {0}")]
    InvalidSkill(String),
    /// The Role has no approved Skill with this name.
    #[error("unknown skill `{0}`")]
    UnknownSkill(String),
    /// The Skill is not a Flow.
    #[error("`{0}` is not a flow")]
    NotAFlow(String),
    /// A Flow tried to start another Flow.
    #[error("a flow cannot start another flow (`{0}`)")]
    FlowCannotStartFlow(String),
    /// There is no earlier version to go back to.
    #[error("`{0}` has no earlier version")]
    NothingToRollBack(String),
    /// The active version's scripts have not been approved.
    #[error("the scripts of `{0}` are not approved")]
    ScriptsNotApproved(String),
    /// The Skill has no script with this name.
    #[error("unknown script `{0}`")]
    UnknownScript(String),
}

/// Procedural knowledge written as a `SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    name: String,
    description: String,
    kind: SkillKind,
    uses: Vec<String>,
    body: String,
    scripts: BTreeMap<String, String>,
}

impl Skill {
    /// Reads a `SKILL.md`: front matter with `name`, `description`, optional
    /// `kind` (`flow` or `discipline`, default discipline) and `uses` (a comma
    /// separated list of Skill names), then the instructions.
    pub fn from_markdown(text: &str) -> Result<Self, SkillError> {
        let bad = |m: &str| SkillError::InvalidSkill(m.into());
        let rest = text
            .strip_prefix("---\n")
            .ok_or_else(|| bad("missing front matter"))?;
        let (front, body) = rest
            .split_once("\n---")
            .ok_or_else(|| bad("unterminated front matter"))?;
        let body = body.strip_prefix('\n').unwrap_or(body);
        let (mut name, mut description) = (None, String::new());
        let (mut kind, mut uses) = (SkillKind::Discipline, Vec::new());
        for line in front.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "name" => name = Some(value.to_string()),
                "description" => description = value.to_string(),
                "kind" => {
                    kind = match value {
                        "flow" => SkillKind::Flow,
                        "discipline" => SkillKind::Discipline,
                        other => return Err(bad(&format!("unknown kind `{other}`"))),
                    }
                }
                "uses" => {
                    uses = value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                }
                _ => {}
            }
        }
        let name = name
            .filter(|n| !n.is_empty())
            .ok_or_else(|| bad("missing name"))?;
        Ok(Self {
            name,
            description,
            kind,
            uses,
            body: body.to_string(),
            scripts: BTreeMap::new(),
        })
    }

    /// Adds a script to the Skill; it stays disabled until separately approved.
    pub fn script(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.scripts.insert(name.into(), source.into());
        self
    }

    /// The Skill's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The one-line description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Flow or Discipline.
    pub fn kind(&self) -> SkillKind {
        self.kind
    }

    /// The names of the Disciplines a Flow uses.
    pub fn uses(&self) -> &[String] {
        &self.uses
    }

    /// The instructions.
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// A new or changed Skill waiting for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSkill {
    /// The Role that would own it.
    pub role: String,
    /// The Skill.
    pub skill: Skill,
}

struct Version {
    number: u32,
    skill: Skill,
    scripts_approved: bool,
}

#[derive(Default)]
struct Entry {
    versions: Vec<Version>,
    /// Index into `versions` of the version in use.
    active: usize,
    shared: bool,
    last_number: u32,
}

#[derive(Default)]
struct Inner {
    pending: Vec<PendingSkill>,
    owned: BTreeMap<(String, String), Entry>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn job(role: &str) -> String {
    format!("skills/{role}")
}

/// Codex's library: holds Skills waiting for approval, installs approved ones,
/// keeps their versions and marks them shared. Cloning gives another handle to
/// the same library.
#[derive(Clone, Default)]
pub struct SkillLibrary {
    inner: Arc<Mutex<Inner>>,
}

impl SkillLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds a new or changed Skill for `role` until a person approves it.
    pub fn propose(&self, role: &str, skill: Skill) {
        lock(&self.inner).pending.push(PendingSkill {
            role: role.into(),
            skill,
        });
    }

    /// The Skills waiting for approval, oldest first.
    pub fn pending(&self) -> Vec<PendingSkill> {
        lock(&self.inner).pending.clone()
    }

    /// Asks `approver` about every pending Skill. Approved ones become the
    /// Role's newest version; rejected ones vanish without a trace; unanswered
    /// ones stay pending.
    pub async fn review_pending(&self, approver: &dyn Approver) {
        let queue = self.pending();
        for item in queue {
            let request = ApprovalRequest {
                job: job(&item.role),
                node: item.skill.name.clone(),
                prompt: format!(
                    "Approve {:?} `{}` for role `{}`",
                    item.skill.kind, item.skill.name, item.role
                ),
            };
            let decision = approver.decide(&request).await;
            let mut inner = lock(&self.inner);
            if decision == ApprovalDecision::Pending {
                continue;
            }
            if let Some(i) = inner.pending.iter().position(|p| *p == item) {
                inner.pending.remove(i);
            }
            if decision == ApprovalDecision::Approved {
                let entry = inner
                    .owned
                    .entry((item.role.clone(), item.skill.name.clone()))
                    .or_default();
                entry.last_number += 1;
                entry.versions.push(Version {
                    number: entry.last_number,
                    skill: item.skill,
                    scripts_approved: false,
                });
                entry.active = entry.versions.len() - 1;
            }
        }
    }

    /// Asks `approver` to approve the scripts of the Role's active version of
    /// `skill`, separately from the Skill itself.
    pub async fn review_scripts(
        &self,
        role: &str,
        skill: &str,
        approver: &dyn Approver,
    ) -> Result<ApprovalDecision, SkillError> {
        let number = {
            let inner = lock(&self.inner);
            let entry = Self::entry(&inner, role, skill)?;
            entry.versions[entry.active].number
        };
        let request = ApprovalRequest {
            job: job(role),
            node: skill.into(),
            prompt: format!("Approve the scripts of `{skill}` version {number} for role `{role}`"),
        };
        let decision = approver.decide(&request).await;
        if decision == ApprovalDecision::Approved {
            let mut inner = lock(&self.inner);
            if let Some(entry) = inner.owned.get_mut(&(role.to_string(), skill.to_string())) {
                if let Some(v) = entry.versions.iter_mut().find(|v| v.number == number) {
                    v.scripts_approved = true;
                }
            }
        }
        Ok(decision)
    }

    /// The source of a script of the Role's active version of `skill`, only if
    /// its scripts have been approved.
    pub fn authorized_script(
        &self,
        role: &str,
        skill: &str,
        script: &str,
    ) -> Result<String, SkillError> {
        let inner = lock(&self.inner);
        let entry = Self::entry(&inner, role, skill)?;
        let v = &entry.versions[entry.active];
        if !v.scripts_approved {
            return Err(SkillError::ScriptsNotApproved(skill.into()));
        }
        v.skill
            .scripts
            .get(script)
            .cloned()
            .ok_or_else(|| SkillError::UnknownScript(script.into()))
    }

    fn entry<'a>(inner: &'a Inner, role: &str, skill: &str) -> Result<&'a Entry, SkillError> {
        inner
            .owned
            .get(&(role.to_string(), skill.to_string()))
            .ok_or_else(|| SkillError::UnknownSkill(skill.into()))
    }

    /// Marks the Role's Skill as shared with every other Role.
    pub fn share(&self, role: &str, skill: &str) -> Result<(), SkillError> {
        self.set_shared(role, skill, true)
    }

    /// Stops sharing the Role's Skill with other Roles.
    pub fn unshare(&self, role: &str, skill: &str) -> Result<(), SkillError> {
        self.set_shared(role, skill, false)
    }

    fn set_shared(&self, role: &str, skill: &str, shared: bool) -> Result<(), SkillError> {
        let mut inner = lock(&self.inner);
        let entry = inner
            .owned
            .get_mut(&(role.to_string(), skill.to_string()))
            .ok_or_else(|| SkillError::UnknownSkill(skill.into()))?;
        entry.shared = shared;
        Ok(())
    }

    /// Goes back to the version before the active one.
    pub fn rollback(&self, role: &str, skill: &str) -> Result<(), SkillError> {
        let mut inner = lock(&self.inner);
        let entry = inner
            .owned
            .get_mut(&(role.to_string(), skill.to_string()))
            .ok_or_else(|| SkillError::UnknownSkill(skill.into()))?;
        if entry.active == 0 {
            return Err(SkillError::NothingToRollBack(skill.into()));
        }
        entry.active -= 1;
        Ok(())
    }

    /// The version numbers kept for the Role's Skill, oldest first.
    pub fn versions(&self, role: &str, skill: &str) -> Vec<u32> {
        let inner = lock(&self.inner);
        Self::entry(&inner, role, skill)
            .map(|e| e.versions.iter().map(|v| v.number).collect())
            .unwrap_or_default()
    }

    /// The approved Skills a Role can use: its own, then those other Roles share
    /// (the Role's own Skill wins when names clash).
    pub fn skills_for_role(&self, role: &str) -> Vec<Skill> {
        let inner = lock(&self.inner);
        let mut out: Vec<Skill> = Vec::new();
        let active = |e: &Entry| e.versions[e.active].skill.clone();
        for ((r, _), e) in &inner.owned {
            if r == role {
                out.push(active(e));
            }
        }
        for ((r, name), e) in &inner.owned {
            if r != role && e.shared && !out.iter().any(|s| s.name == *name) {
                out.push(active(e));
            }
        }
        out
    }

    /// The approved Skills available to `agent`: those of its Role.
    pub fn skills_for_agent(&self, agent: &Agent) -> Vec<Skill> {
        self.skills_for_role(agent.role())
    }

    /// Starts the Role's Flow called `name`.
    pub fn start_flow(&self, role: &str, name: &str) -> Result<FlowRun, SkillError> {
        let skills = self.skills_for_role(role);
        let flow = skills
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .ok_or_else(|| SkillError::UnknownSkill(name.into()))?;
        if flow.kind != SkillKind::Flow {
            return Err(SkillError::NotAFlow(name.into()));
        }
        Ok(FlowRun { flow, skills })
    }
}

/// A running Flow, which may use Disciplines but never start another Flow.
pub struct FlowRun {
    flow: Skill,
    skills: Vec<Skill>,
}

impl FlowRun {
    /// The Flow being run.
    pub fn flow(&self) -> &Skill {
        &self.flow
    }

    /// Uses a Discipline the Role can reach; another Flow is refused.
    pub fn use_skill(&mut self, name: &str) -> Result<Skill, SkillError> {
        let skill = self
            .skills
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| SkillError::UnknownSkill(name.into()))?;
        if skill.kind == SkillKind::Flow {
            return Err(SkillError::FlowCannotStartFlow(name.into()));
        }
        Ok(skill.clone())
    }
}
