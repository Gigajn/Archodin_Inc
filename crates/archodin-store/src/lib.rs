//! Persistent storage for the company.
//!
//! The in-memory stores of `archodin-graph` forget everything when the process
//! ends. This crate keeps the same things in a database so they survive a
//! restart: Checkpoints, Roles, Agents, Skill versions (with Codex's pending
//! queue) and the Akasha.
//!
//! * [`MemoryStore`] keeps everything in memory (tests, short-lived runs).
//! * [`SqlStore`] keeps it in SQLite (feature `sqlite`, for a single machine) or
//!   Postgres (feature `postgres`, for a server); pick one by configuration with
//!   [`SqlStore::connect`].
//!
//! Checkpoints are stored through the graph's own
//! [`CheckpointStore`](archodin_graph::CheckpointStore) trait, so a durable Job
//! runs against a [`SqlStore`] exactly as it does against the in-memory store.
//! Since KR-04, that trait reports errors: a backend failure while saving or
//! reading a Checkpoint ends the durable Job with a terminal
//! `JobEvent::CheckpointFailed` event instead of panicking.
//!
//! # Breaking change (KR-04)
//! `CheckpointStore::save` and `CheckpointStore::checkpoints` now return
//! `Result<_, StoreError>`. All implementers and callers were updated; see
//! `archodin-graph/src/durable.rs`.
//! Roles, Agents, Skills and the Akasha go through [`RosterStore`],
//! [`SkillStore`] and [`AkashaStore`], with the helpers [`save_roster`] /
//! [`restore_roster`], [`save_library`] / [`load_library`] and
//! [`save_akasha`] / [`load_akasha`] bridging them to the in-memory types.
//!
//! What is code is not stored: a Role's Tools and Model chain (which holds API
//! keys) are given back by the caller in [`restore_roster`].

use archodin_graph::{Akasha, Role, Roster, SkillLibrary, SkillSnapshot};
use async_trait::async_trait;

// `StoreError` lives in `archodin-graph` (so the `CheckpointStore` trait can
// name it without a dependency cycle); this crate re-exports it so existing
// `archodin_store::StoreError` paths keep working.
pub use archodin_graph::StoreError;

mod memory;
pub use memory::MemoryStore;

#[cfg(feature = "sql")]
mod sql;
#[cfg(feature = "sql")]
pub use sql::SqlStore;

/// A Role as it is stored: its instructions, Skills slot and Budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRecord {
    /// The Role's name.
    pub name: String,
    /// The Role's instructions.
    pub instructions: String,
    /// The names of the Skills in the Role's Skills slot.
    pub skills: Vec<String>,
    /// The Role's token Budget, if it has one.
    pub budget_tokens: Option<u64>,
}

impl RoleRecord {
    /// What is stored of `role`.
    pub fn from_role(role: &Role) -> Self {
        Self {
            name: role.name().to_string(),
            instructions: role.instructions().to_string(),
            skills: role.skills().to_vec(),
            budget_tokens: role.budget(),
        }
    }

    /// The Role again, without its Tools or Model chain.
    pub fn into_role(self) -> Role {
        let mut role = Role::new(self.name, self.instructions);
        for skill in self.skills {
            role = role.skill(skill);
        }
        if let Some(tokens) = self.budget_tokens {
            role = role.budget_tokens(tokens);
        }
        role
    }
}

/// An Agent as it is stored: its name and the name of its Role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    /// The Agent's unique name.
    pub name: String,
    /// The name of its Role.
    pub role: String,
}

/// Where Roles and Agents are kept.
#[async_trait]
pub trait RosterStore: Send + Sync {
    /// Stores a Role, replacing any Role with the same name.
    async fn save_role(&self, role: &RoleRecord) -> Result<(), StoreError>;
    /// Every Role, ordered by name.
    async fn roles(&self) -> Result<Vec<RoleRecord>, StoreError>;
    /// Stores an Agent after the ones already stored; an Agent with the same
    /// name is replaced and keeps its place.
    async fn save_agent(&self, agent: &AgentRecord) -> Result<(), StoreError>;
    /// Forgets the Agent called `name`; false when there was none.
    async fn remove_agent(&self, name: &str) -> Result<bool, StoreError>;
    /// Every Agent, in the order they were first stored (hired).
    async fn agents(&self) -> Result<Vec<AgentRecord>, StoreError>;
}

/// Where Codex's Skill library is kept: pending Skills and every version.
#[async_trait]
pub trait SkillStore: Send + Sync {
    /// Replaces what is stored with `snapshot`.
    async fn save_skills(&self, snapshot: &SkillSnapshot) -> Result<(), StoreError>;
    /// What is stored (empty when nothing was saved yet).
    async fn load_skills(&self) -> Result<SkillSnapshot, StoreError>;
}

/// Where the Akasha is kept. It has the same rules as
/// [`Akasha`]: a repeated fact costs nothing.
#[async_trait]
pub trait AkashaStore: Send + Sync {
    /// Saves `fact` for `role`; false when the Role already has it (ignoring
    /// case, spacing and closing punctuation) or the fact is empty.
    async fn remember(&self, role: &str, fact: &str) -> Result<bool, StoreError>;
    /// The facts of `role`, oldest first.
    async fn facts(&self, role: &str) -> Result<Vec<String>, StoreError>;
    /// The Roles that have at least one fact, ordered by name.
    async fn fact_roles(&self) -> Result<Vec<String>, StoreError>;
}

/// Everything a company keeps, apart from Checkpoints (see the module docs).
pub trait CompanyStore: RosterStore + SkillStore + AkashaStore {}
impl<T: RosterStore + SkillStore + AkashaStore> CompanyStore for T {}

/// Makes the store match `roster`: every Role is stored, Agents that left are
/// forgotten and new Agents are added after the existing ones.
pub async fn save_roster(store: &dyn RosterStore, roster: &Roster) -> Result<(), StoreError> {
    for role in roster.roles() {
        store.save_role(&RoleRecord::from_role(&role)).await?;
    }
    let current: Vec<AgentRecord> = roster
        .agents()
        .iter()
        .map(|a| AgentRecord {
            name: a.name().to_string(),
            role: a.role().to_string(),
        })
        .collect();
    for stored in store.agents().await? {
        if !current.contains(&stored) {
            store.remove_agent(&stored.name).await?;
        }
    }
    let kept = store.agents().await?;
    for agent in current {
        if !kept.contains(&agent) {
            store.save_agent(&agent).await?;
        }
    }
    Ok(())
}

/// Builds a [`Roster`] from what is stored. `configure` gets each Role before it
/// is added, to give back what is code (Tools, Model chain).
pub async fn restore_roster(
    store: &dyn RosterStore,
    configure: impl Fn(Role) -> Role,
) -> Result<Roster, StoreError> {
    let roster = Roster::new();
    for record in store.roles().await? {
        roster.add_role(configure(record.into_role()));
    }
    for agent in store.agents().await? {
        roster.hire(&agent.role, &agent.name)?;
    }
    Ok(roster)
}

/// Stores everything `library` holds, replacing what was stored.
pub async fn save_library(
    store: &dyn SkillStore,
    library: &SkillLibrary,
) -> Result<(), StoreError> {
    store.save_skills(&library.snapshot()).await
}

/// A [`SkillLibrary`] holding what is stored.
pub async fn load_library(store: &dyn SkillStore) -> Result<SkillLibrary, StoreError> {
    Ok(SkillLibrary::from_snapshot(store.load_skills().await?))
}

/// Stores every fact of `akasha`; facts already stored are left alone.
pub async fn save_akasha(store: &dyn AkashaStore, akasha: &Akasha) -> Result<(), StoreError> {
    for role in akasha.roles() {
        for fact in akasha.facts(&role) {
            store.remember(&role, &fact).await?;
        }
    }
    Ok(())
}

/// An [`Akasha`] holding what is stored.
pub async fn load_akasha(store: &dyn AkashaStore) -> Result<Akasha, StoreError> {
    let akasha = Akasha::new();
    for role in store.fact_roles().await? {
        for fact in store.facts(&role).await? {
            akasha.remember_for_role(&role, &fact);
        }
    }
    Ok(akasha)
}

/// Collapses runs of whitespace; the form in which facts are stored.
pub(crate) fn tidy_fact(fact: &str) -> String {
    fact.split_whitespace().collect::<Vec<_>>().join(" ")
}
