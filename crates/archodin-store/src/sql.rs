//! The SQLite and Postgres backend. Both speak the same SQL here (numbered
//! placeholders, `BIGINT` and `TEXT` columns only), so one implementation on
//! sqlx's `Any` driver serves both.

use std::collections::BTreeMap;
#[cfg(feature = "sqlite")]
use std::path::Path;

use archodin_graph::{
    fact_key, Checkpoint, CheckpointStore, PendingSkill, Skill, SkillEntryRecord, SkillKind,
    SkillSnapshot, SkillVersionRecord,
};
use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};

use crate::{
    sealed_envelope_version, tidy_fact, AgentRecord, AkashaStore, ModelAccountRecord,
    ModelAccountStore, ModelAuditRecord, ModelAuditStore, RoleRecord, RosterStore,
    SealedVaultStore, SkillStore, StoreError,
};

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS checkpoints (
        job TEXT NOT NULL, seq BIGINT NOT NULL, step BIGINT NOT NULL,
        state TEXT NOT NULL, next TEXT,
        PRIMARY KEY (job, seq))",
    "CREATE TABLE IF NOT EXISTS roles (
        name TEXT PRIMARY KEY, instructions TEXT NOT NULL,
        skills TEXT NOT NULL, budget_tokens BIGINT)",
    "CREATE TABLE IF NOT EXISTS agents (
        seq BIGINT NOT NULL, name TEXT PRIMARY KEY, role TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS skill_pending (
        seq BIGINT PRIMARY KEY, role TEXT NOT NULL, summary TEXT,
        unverified BIGINT NOT NULL, skill TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS skill_entries (
        role TEXT NOT NULL, name TEXT NOT NULL, active BIGINT NOT NULL,
        shared BIGINT NOT NULL, last_number BIGINT NOT NULL,
        PRIMARY KEY (role, name))",
    "CREATE TABLE IF NOT EXISTS skill_versions (
        role TEXT NOT NULL, name TEXT NOT NULL, number BIGINT NOT NULL,
        scripts_approved BIGINT NOT NULL, skill TEXT NOT NULL,
        PRIMARY KEY (role, name, number))",
    "CREATE TABLE IF NOT EXISTS akasha (
        role TEXT NOT NULL, fact_key TEXT NOT NULL, seq BIGINT NOT NULL, fact TEXT NOT NULL,
        PRIMARY KEY (role, fact_key))",
    // The sealed vault envelope (ADR 0021): one row, id always 1. The schema
    // version stays 1 on purpose: this statement runs on every `connect`
    // (`CREATE TABLE IF NOT EXISTS`), so existing databases gain the table
    // without a migration, and bumping the version would reject them.
    "CREATE TABLE IF NOT EXISTS sealed_vault (
        id BIGINT PRIMARY KEY, version BIGINT NOT NULL,
        sealed BYTEA NOT NULL, updated_at BIGINT NOT NULL)",
    // Model-account metadata (ADR 0023): NON-secret only — the key NEVER
    // enters this table. TEXT throughout, so the same statement serves
    // SQLite and Postgres; no BLOB column. `budget_tokens` / `budget_cost`
    // are the account's optional caps (ADR 0024 §Budget).
    "CREATE TABLE IF NOT EXISTS model_accounts (
        name TEXT PRIMARY KEY, kind TEXT NOT NULL,
        base_url TEXT NOT NULL, default_model TEXT NOT NULL,
        budget_tokens BIGINT, budget_cost REAL)",
    // The model audit log (ADR 0024): append-only, newest first by `seq`.
    // No key value, no part of one, no raw upstream body ever lands here
    // — the writer redacts `result` before recording.
    "CREATE TABLE IF NOT EXISTS model_audit (
        seq BIGINT PRIMARY KEY, at BIGINT NOT NULL,
        actor TEXT NOT NULL, action TEXT NOT NULL, kind TEXT NOT NULL,
        account TEXT NOT NULL, model TEXT NOT NULL, result TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS schema_meta (
        key TEXT PRIMARY KEY, value TEXT NOT NULL)",
];

/// Schema version of the tables above. Bump when the schema changes and
/// add the matching migration next to the check in `connect`.
const SCHEMA_VERSION: i64 = 1;

/// Everything a company keeps, in SQLite or Postgres. Cloning gives another
/// handle to the same database.
#[derive(Clone)]
pub struct SqlStore {
    pool: AnyPool,
}

fn backend(e: sqlx::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

fn invalid(e: impl std::fmt::Display) -> StoreError {
    StoreError::Invalid(e.to_string())
}

impl SqlStore {
    /// Connects to the database `url` names (`sqlite:...` or `postgres://...`,
    /// as far as the matching feature is enabled) and creates the tables that
    /// are missing. This is how a deployment picks its backend by configuration.
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        sqlx::any::install_default_drivers();
        // Every connection to an in-memory SQLite database is a database of its own.
        let max = if url.contains(":memory:") { 1 } else { 5 };
        let pool = AnyPoolOptions::new()
            .max_connections(max)
            .connect(url)
            .await
            .map_err(backend)?;
        for statement in SCHEMA {
            sqlx::query(*statement)
                .execute(&pool)
                .await
                .map_err(backend)?;
        }
        // Best-effort migration for databases created before the budget
        // columns existed (Fase 3b-D): `CREATE TABLE IF NOT EXISTS` above
        // does not add columns to an existing table. Neither SQLite nor
        // Postgres spells "add if missing" the same way, so the plain
        // `ALTER TABLE` is run and its error ignored — on a database that
        // already has the column the only failure is "duplicate column",
        // and on a fresh database the column came from `CREATE TABLE`.
        // A failure here therefore never hides a real problem: the next
        // write to the column would fail loudly instead.
        for alter in [
            "ALTER TABLE model_accounts ADD COLUMN budget_tokens BIGINT",
            "ALTER TABLE model_accounts ADD COLUMN budget_cost REAL",
        ] {
            let _ = sqlx::query(alter).execute(&pool).await;
        }
        // Stamp the version on fresh databases; refuse a database whose schema
        // this build does not understand instead of reading it as if it did.
        // The upsert is race-safe: two first `connect()`s running together
        // both stamp v1, and only one row wins.
        let version: Option<String> =
            sqlx::query("SELECT value FROM schema_meta WHERE key = 'version'")
                .fetch_optional(&pool)
                .await
                .map_err(backend)?
                .map(|r| r.try_get::<String, _>("value"))
                .transpose()
                .map_err(backend)?;
        match version.as_deref() {
            None => {
                sqlx::query(
                    "INSERT INTO schema_meta (key, value) VALUES ('version', $1)
                     ON CONFLICT (key) DO NOTHING",
                )
                .bind(SCHEMA_VERSION.to_string())
                .execute(&pool)
                .await
                .map_err(backend)?;
            }
            Some(v) if v == SCHEMA_VERSION.to_string() => {}
            Some(v) => {
                return Err(StoreError::Invalid(format!(
                    "unsupported schema version {v} (this build expects {SCHEMA_VERSION})"
                )));
            }
        }
        Ok(Self { pool })
    }

    /// Opens (creating it when missing) the SQLite database file at `path`.
    #[cfg(feature = "sqlite")]
    pub async fn sqlite_file(path: &Path) -> Result<Self, StoreError> {
        Self::connect(&format!("sqlite://{}?mode=rwc", path.display())).await
    }

    /// A private in-memory SQLite database that lives as long as this store.
    #[cfg(feature = "sqlite")]
    pub async fn sqlite_in_memory() -> Result<Self, StoreError> {
        Self::connect("sqlite::memory:").await
    }

    async fn save_checkpoint(
        &self,
        job: &str,
        step: usize,
        state: &str,
        next: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let last: Option<i64> =
            sqlx::query("SELECT MAX(seq) AS last FROM checkpoints WHERE job = $1")
                .bind(job)
                .fetch_one(&mut *tx)
                .await
                .map_err(backend)?
                .try_get("last")
                .map_err(backend)?;
        sqlx::query(
            "INSERT INTO checkpoints (job, seq, step, state, next) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(job)
        .bind(last.unwrap_or(0) + 1)
        .bind(step as i64)
        .bind(state)
        .bind(next)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    async fn load_checkpoints(
        &self,
        job: &str,
    ) -> Result<Vec<(usize, String, Option<String>)>, StoreError> {
        let rows =
            sqlx::query("SELECT step, state, next FROM checkpoints WHERE job = $1 ORDER BY seq")
                .bind(job)
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.try_get::<i64, _>("step").map_err(backend)? as usize,
                    r.try_get("state").map_err(backend)?,
                    r.try_get("next").map_err(backend)?,
                ))
            })
            .collect()
    }
}

impl<S> CheckpointStore<S> for SqlStore
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn save(&self, checkpoint: Checkpoint<S>) -> BoxFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let state = serde_json::to_string(&checkpoint.state).map_err(invalid)?;
            self.save_checkpoint(
                &checkpoint.job,
                checkpoint.step,
                &state,
                checkpoint.next.as_deref(),
            )
            .await
        })
    }

    fn checkpoints(&self, job: &str) -> BoxFuture<'_, Result<Vec<Checkpoint<S>>, StoreError>> {
        let job = job.to_string();
        Box::pin(async move {
            let rows = self.load_checkpoints(&job).await?;
            rows.into_iter()
                .map(|(step, state, next)| {
                    let state: S = serde_json::from_str(&state).map_err(invalid)?;
                    Ok(Checkpoint {
                        job: job.clone(),
                        step,
                        state,
                        next,
                    })
                })
                .collect::<Result<Vec<Checkpoint<S>>, StoreError>>()
        })
    }
}

#[async_trait]
impl RosterStore for SqlStore {
    async fn save_role(&self, role: &RoleRecord) -> Result<(), StoreError> {
        let skills = serde_json::to_string(&role.skills).map_err(invalid)?;
        let budget = role.budget_tokens.map(|b| b as i64);
        sqlx::query(
            "INSERT INTO roles (name, instructions, skills, budget_tokens) VALUES ($1, $2, $3, $4)
             ON CONFLICT (name) DO UPDATE SET instructions = excluded.instructions,
                skills = excluded.skills, budget_tokens = excluded.budget_tokens",
        )
        .bind(&role.name)
        .bind(&role.instructions)
        .bind(skills)
        .bind(budget)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn roles(&self) -> Result<Vec<RoleRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT name, instructions, skills, budget_tokens FROM roles ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let skills: String = r.try_get("skills").map_err(backend)?;
                let budget: Option<i64> = r.try_get("budget_tokens").map_err(backend)?;
                Ok(RoleRecord {
                    name: r.try_get("name").map_err(backend)?,
                    instructions: r.try_get("instructions").map_err(backend)?,
                    skills: serde_json::from_str(&skills).map_err(invalid)?,
                    budget_tokens: budget.map(|b| b as u64),
                })
            })
            .collect()
    }

    async fn save_agent(&self, agent: &AgentRecord) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let updated = sqlx::query("UPDATE agents SET role = $1 WHERE name = $2")
            .bind(&agent.role)
            .bind(&agent.name)
            .execute(&mut *tx)
            .await
            .map_err(backend)?
            .rows_affected();
        if updated == 0 {
            let last: Option<i64> = sqlx::query("SELECT MAX(seq) AS last FROM agents")
                .fetch_one(&mut *tx)
                .await
                .map_err(backend)?
                .try_get("last")
                .map_err(backend)?;
            sqlx::query("INSERT INTO agents (seq, name, role) VALUES ($1, $2, $3)")
                .bind(last.unwrap_or(0) + 1)
                .bind(&agent.name)
                .bind(&agent.role)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
        }
        tx.commit().await.map_err(backend)
    }

    async fn remove_agent(&self, name: &str) -> Result<bool, StoreError> {
        let removed = sqlx::query("DELETE FROM agents WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(backend)?
            .rows_affected();
        Ok(removed > 0)
    }

    async fn agents(&self) -> Result<Vec<AgentRecord>, StoreError> {
        let rows = sqlx::query("SELECT name, role FROM agents ORDER BY seq")
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter()
            .map(|r| {
                Ok(AgentRecord {
                    name: r.try_get("name").map_err(backend)?,
                    role: r.try_get("role").map_err(backend)?,
                })
            })
            .collect()
    }
}

/// A Skill as stored: its parts as JSON, so no `SKILL.md` round trip can lose any.
#[derive(Serialize, Deserialize)]
struct SkillDoc {
    name: String,
    description: String,
    kind: String,
    uses: Vec<String>,
    body: String,
    scripts: BTreeMap<String, String>,
}

impl SkillDoc {
    fn encode(skill: &Skill) -> Result<String, StoreError> {
        let doc = SkillDoc {
            name: skill.name().into(),
            description: skill.description().into(),
            kind: match skill.kind() {
                SkillKind::Flow => "flow",
                SkillKind::Discipline => "discipline",
            }
            .into(),
            uses: skill.uses().to_vec(),
            body: skill.body().into(),
            scripts: skill.scripts().clone(),
        };
        serde_json::to_string(&doc).map_err(invalid)
    }

    fn decode(text: &str) -> Result<Skill, StoreError> {
        let doc: SkillDoc = serde_json::from_str(text).map_err(invalid)?;
        let kind = match doc.kind.as_str() {
            "flow" => SkillKind::Flow,
            "discipline" => SkillKind::Discipline,
            other => return Err(invalid(format!("unknown skill kind `{other}`"))),
        };
        Ok(Skill::from_parts(
            doc.name,
            doc.description,
            kind,
            doc.uses,
            doc.body,
            doc.scripts,
        ))
    }
}

#[async_trait]
impl SkillStore for SqlStore {
    async fn save_skills(&self, snapshot: &SkillSnapshot) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        for statement in [
            "DELETE FROM skill_pending",
            "DELETE FROM skill_entries",
            "DELETE FROM skill_versions",
        ] {
            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
        }
        for (seq, p) in snapshot.pending.iter().enumerate() {
            sqlx::query(
                "INSERT INTO skill_pending (seq, role, summary, unverified, skill)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(seq as i64)
            .bind(&p.role)
            .bind(p.summary.as_deref())
            .bind(i64::from(p.unverified))
            .bind(SkillDoc::encode(&p.skill)?)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
        for e in &snapshot.owned {
            sqlx::query(
                "INSERT INTO skill_entries (role, name, active, shared, last_number)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&e.role)
            .bind(&e.name)
            .bind(e.active as i64)
            .bind(i64::from(e.shared))
            .bind(i64::from(e.last_number))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            for v in &e.versions {
                sqlx::query(
                    "INSERT INTO skill_versions (role, name, number, scripts_approved, skill)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(&e.role)
                .bind(&e.name)
                .bind(i64::from(v.number))
                .bind(i64::from(v.scripts_approved))
                .bind(SkillDoc::encode(&v.skill)?)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
        }
        tx.commit().await.map_err(backend)
    }

    async fn load_skills(&self) -> Result<SkillSnapshot, StoreError> {
        let pending_rows =
            sqlx::query("SELECT role, summary, unverified, skill FROM skill_pending ORDER BY seq")
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        let mut pending = Vec::new();
        for r in &pending_rows {
            let unverified: i64 = r.try_get("unverified").map_err(backend)?;
            let skill: String = r.try_get("skill").map_err(backend)?;
            pending.push(PendingSkill {
                role: r.try_get("role").map_err(backend)?,
                skill: SkillDoc::decode(&skill)?,
                summary: r.try_get("summary").map_err(backend)?,
                unverified: unverified != 0,
            });
        }
        let entry_rows = sqlx::query(
            "SELECT role, name, active, shared, last_number FROM skill_entries ORDER BY role, name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut owned = Vec::new();
        for r in &entry_rows {
            let role: String = r.try_get("role").map_err(backend)?;
            let name: String = r.try_get("name").map_err(backend)?;
            let version_rows = sqlx::query(
                "SELECT number, scripts_approved, skill FROM skill_versions
                 WHERE role = $1 AND name = $2 ORDER BY number",
            )
            .bind(&role)
            .bind(&name)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
            let mut versions = Vec::new();
            for v in &version_rows {
                let number: i64 = v.try_get("number").map_err(backend)?;
                let approved: i64 = v.try_get("scripts_approved").map_err(backend)?;
                let skill: String = v.try_get("skill").map_err(backend)?;
                versions.push(SkillVersionRecord {
                    number: u32::try_from(number).map_err(invalid)?,
                    skill: SkillDoc::decode(&skill)?,
                    scripts_approved: approved != 0,
                });
            }
            let active: i64 = r.try_get("active").map_err(backend)?;
            let shared: i64 = r.try_get("shared").map_err(backend)?;
            let last: i64 = r.try_get("last_number").map_err(backend)?;
            owned.push(SkillEntryRecord {
                role,
                name,
                versions,
                active: usize::try_from(active).map_err(invalid)?,
                shared: shared != 0,
                last_number: u32::try_from(last).map_err(invalid)?,
            });
        }
        Ok(SkillSnapshot { pending, owned })
    }
}

#[async_trait]
impl AkashaStore for SqlStore {
    async fn remember(&self, role: &str, fact: &str) -> Result<bool, StoreError> {
        let fact = tidy_fact(fact);
        let key = fact_key(&fact);
        if key.is_empty() {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let last: Option<i64> = sqlx::query("SELECT MAX(seq) AS last FROM akasha WHERE role = $1")
            .bind(role)
            .fetch_one(&mut *tx)
            .await
            .map_err(backend)?
            .try_get("last")
            .map_err(backend)?;
        let added = sqlx::query(
            "INSERT INTO akasha (role, fact_key, seq, fact) VALUES ($1, $2, $3, $4)
             ON CONFLICT (role, fact_key) DO NOTHING",
        )
        .bind(role)
        .bind(key)
        .bind(last.unwrap_or(0) + 1)
        .bind(fact)
        .execute(&mut *tx)
        .await
        .map_err(backend)?
        .rows_affected();
        tx.commit().await.map_err(backend)?;
        Ok(added > 0)
    }

    async fn facts(&self, role: &str) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query("SELECT fact FROM akasha WHERE role = $1 ORDER BY seq")
            .bind(role)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter()
            .map(|r| r.try_get("fact").map_err(backend))
            .collect()
    }

    async fn fact_roles(&self) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query("SELECT DISTINCT role FROM akasha ORDER BY role")
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter()
            .map(|r| r.try_get("role").map_err(backend))
            .collect()
    }
}

/// Seconds since the Unix epoch, for the `updated_at` column. Never fails:
/// a clock before the epoch stamps 0 rather than failing the save.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64
}

#[async_trait]
impl SealedVaultStore for SqlStore {
    async fn save_sealed_vault(&self, envelope: &[u8]) -> Result<(), StoreError> {
        let version = sealed_envelope_version(envelope)?;
        sqlx::query(
            "INSERT INTO sealed_vault (id, version, sealed, updated_at) VALUES (1, $1, $2, $3)
             ON CONFLICT (id) DO UPDATE SET version = excluded.version,
                sealed = excluded.sealed, updated_at = excluded.updated_at",
        )
        .bind(version)
        .bind(envelope)
        .bind(unix_now())
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn load_sealed_vault(&self) -> Result<Option<Vec<u8>>, StoreError> {
        let row = sqlx::query("SELECT sealed FROM sealed_vault WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)?;
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(r.try_get("sealed").map_err(backend)?)),
        }
    }
}

#[async_trait]
impl ModelAccountStore for SqlStore {
    async fn save_model_account(&self, account: &ModelAccountRecord) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO model_accounts (name, kind, base_url, default_model, budget_tokens, budget_cost)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (name) DO UPDATE SET kind = excluded.kind,
                base_url = excluded.base_url, default_model = excluded.default_model,
                budget_tokens = excluded.budget_tokens, budget_cost = excluded.budget_cost",
        )
        .bind(&account.name)
        .bind(&account.kind)
        .bind(&account.base_url)
        .bind(&account.default_model)
        .bind(account.budget_tokens)
        .bind(account.budget_cost)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn load_model_accounts(&self) -> Result<Vec<ModelAccountRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT name, kind, base_url, default_model, budget_tokens, budget_cost
             FROM model_accounts ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                Ok(ModelAccountRecord {
                    name: r.try_get("name").map_err(backend)?,
                    kind: r.try_get("kind").map_err(backend)?,
                    base_url: r.try_get("base_url").map_err(backend)?,
                    default_model: r.try_get("default_model").map_err(backend)?,
                    budget_tokens: r.try_get("budget_tokens").map_err(backend)?,
                    budget_cost: r.try_get("budget_cost").map_err(backend)?,
                })
            })
            .collect()
    }

    async fn remove_model_account(&self, name: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM model_accounts WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }
}

#[async_trait]
impl ModelAuditStore for SqlStore {
    async fn record_audit(&self, record: &ModelAuditRecord) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let last: Option<i64> = sqlx::query("SELECT MAX(seq) AS last FROM model_audit")
            .fetch_one(&mut *tx)
            .await
            .map_err(backend)?
            .try_get("last")
            .map_err(backend)?;
        sqlx::query(
            "INSERT INTO model_audit (seq, at, actor, action, kind, account, model, result)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(last.unwrap_or(0) + 1)
        .bind(record.at)
        .bind(&record.actor)
        .bind(&record.action)
        .bind(&record.kind)
        .bind(&record.account)
        .bind(&record.model)
        .bind(&record.result)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    async fn list_audit(&self, limit: usize) -> Result<Vec<ModelAuditRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT seq, at, actor, action, kind, account, model, result
             FROM model_audit ORDER BY seq DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                Ok(ModelAuditRecord {
                    seq: r.try_get("seq").map_err(backend)?,
                    at: r.try_get("at").map_err(backend)?,
                    actor: r.try_get("actor").map_err(backend)?,
                    action: r.try_get("action").map_err(backend)?,
                    kind: r.try_get("kind").map_err(backend)?,
                    account: r.try_get("account").map_err(backend)?,
                    model: r.try_get("model").map_err(backend)?,
                    result: r.try_get("result").map_err(backend)?,
                })
            })
            .collect()
    }
}
