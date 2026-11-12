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
        // `Role::new` starts at the default Budget, so a stored `None` must
        // opt out explicitly; otherwise every restored Role without a Budget
        // silently regained the 100k default.
        match self.budget_tokens {
            Some(tokens) => role = role.budget_tokens(tokens),
            None => role = role.without_budget_limit(),
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

/// Where the sealed vault envelope (ADR 0021) is kept: one row, id 1, in the
/// `sealed_vault` table (`version`, `sealed` BLOB, `updated_at`). This is
/// deliberately NOT part of [`CompanyStore`]: the envelope is credential
/// material, not company data, and only the startup/restore path touches it.
#[async_trait]
pub trait SealedVaultStore: Send + Sync {
    /// Stores `envelope` (bytes from `archodin_graph::Vault::seal_envelope`),
    /// replacing whatever was stored. The envelope's `version` is read from
    /// the JSON and stored in the `version` column; bytes that are not a
    /// versioned envelope are rejected with [`StoreError::Invalid`], so a
    /// caller can never silently persist the wrong thing.
    async fn save_sealed_vault(&self, envelope: &[u8]) -> Result<(), StoreError>;
    /// The stored envelope, or `None` when nothing was saved yet.
    async fn load_sealed_vault(&self) -> Result<Option<Vec<u8>>, StoreError>;
}

/// One model account as it is stored (ADR 0023): NON-secret metadata only.
/// The key NEVER enters this record — it lives in the Vault under
/// `model/<kind>/<name>`, and this table only says the account exists, what
/// it speaks, and which model it defaults to.
///
/// `budget_tokens` / `budget_cost` are the account's OPTIONAL caps
/// (ADR 0024 §Budget): `None` means no cap on that side. They are enforced
/// through [`archodin_graph::UsageMeter`] via the `ModelBudgets` seam
/// (`archodin-graph/src/model_ops.rs`), never by a second metering system.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelAccountRecord {
    /// The account name, for example `main` or `groq_cepat`.
    pub name: String,
    /// The kind spelling: `openai_compatible`, `gemini`, `claude`, `ollama`.
    pub kind: String,
    /// The API root, for example `https://api.openai.com/v1`.
    pub base_url: String,
    /// The model used when a chain names this account without a model of
    /// its own. May be empty when the model is set per Role instead.
    pub default_model: String,
    /// The most tokens the account may spend (`None` = no cap).
    pub budget_tokens: Option<i64>,
    /// The most money the account may spend (`None` = no cap).
    pub budget_cost: Option<f64>,
}

/// Where model-account metadata (ADR 0023) is kept, in the `model_accounts`
/// table (`name TEXT PRIMARY KEY, kind TEXT NOT NULL, base_url TEXT NOT
/// NULL, default_model TEXT NOT NULL, budget_tokens BIGINT,
/// budget_cost REAL`). Deliberately NOT part of
/// [`CompanyStore`]: like the sealed vault, this is credential-adjacent
/// configuration, touched only by the startup/restore path and the Agora
/// model API — and the key is never in this table, by construction.
#[async_trait]
pub trait ModelAccountStore: Send + Sync {
    /// Stores `account`, replacing any account with the same name.
    async fn save_model_account(&self, account: &ModelAccountRecord) -> Result<(), StoreError>;
    /// Every stored account, ordered by name.
    async fn load_model_accounts(&self) -> Result<Vec<ModelAccountRecord>, StoreError>;
    /// Forgets the account called `name`; succeeds when there was none.
    async fn remove_model_account(&self, name: &str) -> Result<(), StoreError>;
}

/// One row of the append-only audit log (ADR 0024): a key-lifecycle event
/// or an auth failure. This record NEVER holds a key value, part of one,
/// or a raw upstream error body — every free-text field (`result`) is
/// passed through `Vault::redact` before it is recorded (see
/// `archodin-graph/src/model_ops.rs`, `AuditEvent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAuditRecord {
    /// Row number, assigned by the store; pass 0 when recording.
    pub seq: i64,
    /// When the event happened (seconds since the Unix epoch).
    pub at: i64,
    /// Who did it (`"admin"` in v1 — see `AUDIT_ACTOR`).
    pub actor: String,
    /// `provision` | `rotate` | `revoke` | `health_check` | `auth_failed`
    /// | `call_failed`.
    pub action: String,
    /// Provider kind spelling (`openai_compatible`, ...).
    pub kind: String,
    /// Account name.
    pub account: String,
    /// Default model name of the account (may be empty).
    pub model: String,
    /// Outcome, already redacted: `ok`, `auth_failed`, `unreachable`,
    /// `key_missing`, `not_persisted: <reason>`, `persist_failed: <reason>`.
    pub result: String,
}

/// Where the model audit log (ADR 0024) is kept, in the `model_audit`
/// table. Append-only: rows are never updated or deleted through this
/// trait. Deliberately NOT part of [`CompanyStore`]: like the sealed
/// vault, this is credential-adjacent data, touched only by the Agora
/// model API. Follows the `SealedVaultStore` pattern (a separate trait,
/// implemented by both backends).
#[async_trait]
pub trait ModelAuditStore: Send + Sync {
    /// Appends `record` (its `seq` is assigned by the store).
    async fn record_audit(&self, record: &ModelAuditRecord) -> Result<(), StoreError>;
    /// The newest events first, at most `limit` rows.
    async fn list_audit(&self, limit: usize) -> Result<Vec<ModelAuditRecord>, StoreError>;
}

/// Everything the Agora model API persists through one handle: account
/// metadata, the sealed vault envelope, and the audit log. Rust trait
/// objects allow only one non-auto trait, so this combined trait is the
/// single object type (`Arc<dyn ModelStore>`) the console holds; both
/// backends implement all three parts, so the blanket impl covers them.
pub trait ModelStore: ModelAccountStore + SealedVaultStore + ModelAuditStore {}
impl<T: ModelAccountStore + SealedVaultStore + ModelAuditStore> ModelStore for T {}

/// The `version` field of a sealed-vault envelope, for the `version` column
/// of the `sealed_vault` table. Only the shape is checked here (a JSON object
/// with a numeric `version`); whether that version is understood is decided
/// when the envelope is opened, not when it is stored.
pub(crate) fn sealed_envelope_version(envelope: &[u8]) -> Result<i64, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(envelope).map_err(|_| {
        StoreError::Invalid("sealed vault bytes are not a versioned envelope".to_string())
    })?;
    value
        .get("version")
        .and_then(|v| v.as_u64())
        .and_then(|v| i64::try_from(v).ok())
        .ok_or_else(|| {
            StoreError::Invalid("sealed vault envelope has no numeric `version`".to_string())
        })
}

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
