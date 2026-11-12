//! Assembles the running company: SqlStore + Roster + Graph + Agora.
//!
//! `assemble` builds everything the server needs WITHOUT binding a socket,
//! so the smoke test (`tests/assembly_smoke.rs`) can exercise it. `main`
//! only binds the socket and serves the router `assemble` returns.
//!
//! # What this wires, and what it deliberately does NOT wire yet
//!
//! * The SqlStore is opened (SQLite file, or in-memory) and the Roster is
//!   restored from it — or seeded with the eight company Roles and saved —
//!   so a restart keeps the company.
//! * The Graph is a durable job machine: every Job runs through
//!   `Graph::stream_job` against the SqlStore as its `CheckpointStore`, so
//!   progress survives a restart, and one node asks the Agora for approval
//!   before it runs, so the human-in-the-loop path is real.
//! * The Agora is built WITH auth (KR-07): no credentials, no server.
//!
//! What is NOT wired yet, on purpose: the Roles have no Tools and no LLM
//! model chains. A Role's Tools and chain are code (and hold API keys), so
//! they cannot come from the store — `restore_roster` says so itself — and
//! choosing a provider plus its keys is a separate, credential-bearing
//! decision. Until that lands, Job nodes are deterministic functions on the
//! company state, not LLM agents. The assembly (Agora + Graph + Roster +
//! SqlStore) is real; the agents' brains are the documented follow-up.
//!
//! # API status — verified against `main` @ `520e6e17`
//!
//! The call sites below were verified line-by-line against the code actually
//! read from `main` (2026-11-10): `SqlStore::connect` / `sqlite_file` /
//! `sqlite_in_memory` (`archodin-store/src/sql.rs`), `Graph::builder()` +
//! `.entry()` / `.node()` / `.node_with_approval()` / `.edge()` / `.build()
//! -> Result<Graph<S>, GraphError>` (`archodin-graph/src/lib.rs`,
//! `src/durable.rs`), `Graph::stream_job` / `run_job`
//! (`archodin-graph/src/durable.rs`), `pump` / `JobContext` /
//! `Agora::builder()` / `.auth()` / `.runner()` / `.build()` / `.router()`
//! (`archodin-console/src/lib.rs`), `hash_password(&str) -> String` and
//! `Auth::from_vault` (`archodin-console/src/auth.rs`), `Vault::new` /
//! `put` (`archodin-graph/src/vault.rs`), `Roster::new` / `add_role` /
//! `hire` and `Role::new` (`archodin-graph/src/roster.rs`),
//! `save_roster` / `restore_roster` (`archodin-store/src/lib.rs`).
//!
//! Merge order: KR-07 makes Agora auth mandatory by default (its branch
//! changes `AgoraBuilder::build()` to panic without `.auth(...)`, and adds
//! login lockout + job quotas). This crate already passes `.auth(...)`, and
//! `Auth::from_vault`'s signature is identical in `main` and on the KR-07
//! branch, so the auth call site needs no change either way — but rebase
//! onto KR-07 (or merge KR-07 first) before merging this crate, so the
//! lockout and quota guarantees actually hold for the running server.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use archodin_console::{hash_password, pump, Agora, Auth, JobContext, ModelApi};
use archodin_graph::{
    Graph, ModelAccount, ModelError, ModelKind, Registry, Role, Roster, RosterError, UsageMeter,
    Vault, VaultError,
};
use archodin_store::{
    restore_roster, save_roster, ModelAccountStore, ModelStore, RosterStore, SealedVaultStore,
    SqlStore, StoreError,
};
use axum::Router;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::config::{Config, DbConfig};

/// The state one Job carries through the Graph.
///
/// It must be `Debug` (`pump`), `Clone + Send + 'static`
/// (`Graph::stream_job`) and `Serialize + DeserializeOwned` (the SqlStore's
/// `CheckpointStore` implementation) — all four are derived here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyState {
    /// What the person asked for (the Job's request text).
    pub request: String,
    /// A short log of what the Job did, step by step.
    pub notes: Vec<String>,
    /// What the Job produced, once it finished.
    pub outcome: Option<String>,
}

impl CompanyState {
    /// The initial state for a Job submitted through the Agora.
    fn from_request(request: &str) -> Self {
        Self {
            request: request.to_string(),
            notes: Vec::new(),
            outcome: None,
        }
    }
}

/// The eight company Roles, with the instructions each one works under.
/// (Role names and one-line mandates follow the project spec, issue #2.)
const ROLES: [(&str, &str); 8] = [
    (
        "Strategos",
        "Supervisor: plans the work, splits it into jobs, and checks the result.",
    ),
    ("Nakama", "HR: hires and manages the company's agents."),
    ("Ronin", "Finds new skills for the company to learn."),
    ("Vigil", "Inspects skills for safety before they are used."),
    (
        "Codex",
        "Curates the skill library: versions, sharing, rollback.",
    ),
    ("Explorador", "Builds and changes the company's own code."),
    ("Forja", "Research and development."),
    ("Qadi", "Settles conflicts between agents."),
];

/// The assembled application: everything `main` needs to serve.
pub struct App {
    /// The Agora (also the `Approver` the runner passes to the Graph).
    pub agora: Agora,
    /// The HTTP router; `main` serves this and nothing else.
    pub router: Router,
    /// The open store (checkpoints, roster, skills, Akasha).
    pub store: SqlStore,
    /// The company's roles and agents.
    pub roster: Roster,
    /// The Vault holding every model key (and the Agora login, under
    /// `agora/*`). Keys are never copied out of here except per call.
    pub vault: Vault,
    /// The Model Registry: model accounts (kind, base URL, default model,
    /// Vault secret name) — NON-secret data only, keys never enter it.
    /// Shared (`Arc<Mutex<..>>`) with the Agora's Model API, so accounts
    /// provisioned through the API and accounts read here never drift.
    pub registry: Arc<Mutex<Registry>>,
    /// True when a sealed vault exists but no Vault key was configured, so
    /// its accounts could not be opened (ADR 0021 §Decision 4). The server
    /// still starts; the locked accounts are named in `locked_accounts`.
    pub vault_locked: bool,
    /// Account names whose key is not in the Vault: sealed accounts in a
    /// locked vault, and table accounts whose key was never provisioned.
    pub locked_accounts: Vec<String>,
}

/// Why assembly failed. No `unwrap`/`expect` anywhere in this module:
/// a failure to open the database or build the Agora is an error, not a panic.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Configuration was missing or invalid.
    #[error("configuration: {0}")]
    Config(#[from] crate::config::ConfigError),
    /// The store could not be opened, read, or written.
    #[error("storage: {0}")]
    Store(#[from] StoreError),
    /// The roster could not be restored or seeded.
    #[error("roster: {0}")]
    Roster(#[from] RosterError),
    /// The Agora auth could not be built.
    #[error("auth: {0}")]
    Auth(#[from] archodin_console::AuthError),
    /// A fresh random Vault key could not be generated.
    #[error("random key: {0}")]
    Random(#[from] getrandom::Error),
    /// The sealed vault could not be opened (wrong key, tampered bytes,
    /// unknown envelope version) — the server refuses to start with a
    /// half-filled Vault rather than silently losing keys.
    #[error("vault: {0}")]
    Vault(#[from] VaultError),
    /// A model account could not be registered (bad shape in the store).
    #[error("model registry: {0}")]
    Model(#[from] ModelError),
    /// The Graph could not be built (bad entry, edge, or duplicate node).
    #[error("graph: {0}")]
    Graph(#[from] archodin_graph::GraphError),
    /// Serving failed (bind, I/O).
    #[error("server: {0}")]
    Io(#[from] std::io::Error),
}

/// Builds the durable job machine.
///
/// `intake` records the request, `acknowledge` asks the operator (through
/// the Agora) to confirm the job should proceed, and `finish` writes the
/// outcome. Every step is checkpointed to the SqlStore by `stream_job`,
/// so a Job resumed under the same id continues where it stopped.
///
/// Public so tests can drive the job machine (including its approval
/// round-trip) without starting the server; `assemble` is the only
/// production caller.
pub fn build_graph() -> Result<Graph<CompanyState>, Error> {
    // `Graph::builder()` + `.entry("intake")` — verified against
    // `archodin-graph/src/lib.rs:289,192` (`GraphBuilder` has no `new`).
    let graph = Graph::builder()
        .entry("intake")
        .node("intake", |mut state: CompanyState| async move {
            state.notes.push("intake: request recorded".to_string());
            state
        })
        .node_with_approval(
            "acknowledge",
            |state: &CompanyState| format!("Proceed with this job? Request: {}", state.request),
            |mut state: CompanyState| async move {
                state
                    .notes
                    .push("acknowledge: operator approved".to_string());
                state
            },
        )
        .node("finish", |mut state: CompanyState| async move {
            state.outcome = Some(format!("done: {}", state.request));
            state
        })
        .edge("intake", "acknowledge")
        .edge("acknowledge", "finish")
        // `build()` validates entry/edge/duplicates and returns
        // `Result<Graph<S>, GraphError>` (`archodin-graph/src/lib.rs:229`).
        .build()?;
    Ok(graph)
}

/// Restores the Roster from the store, or seeds it with the eight company
/// Roles (and one agent per role) and saves it, on a fresh database.
async fn build_roster(store: &SqlStore) -> Result<Roster, Error> {
    if !store.roles().await?.is_empty() {
        // Roles already seeded: restore them. The `configure` closure gives
        // back what is code (Tools, model chains) — this binary has none
        // yet, so it is the identity; see the module docs.
        return Ok(restore_roster(store, |role| role).await?);
    }
    let roster = Roster::new();
    for (name, instructions) in ROLES {
        roster.add_role(Role::new(name, instructions));
    }
    for (name, _) in ROLES {
        roster.hire(name, &name.to_lowercase())?;
    }
    save_roster(store, &roster).await?;
    Ok(roster)
}

/// The account names of the `model/*` secrets visible in a sealed-vault
/// envelope, without the Vault key: only the NAMES are in clear in the
/// envelope (ADR 0021), never the values. A malformed envelope logs an
/// error and yields no names — the caller then reports an empty lock list
/// rather than guessing. The `sealed` field is the raw sealed bytes
/// (a JSON array of byte values); decoding it as JSON gives the
/// name → ciphertext map.
fn locked_account_names_from_envelope(envelope: &[u8]) -> Vec<String> {
    let malformed = || {
        eprintln!(
            "basileia: WARNING: sealed vault envelope is malformed; \
             locked model accounts cannot be listed"
        );
        Vec::new()
    };
    let value: serde_json::Value = match serde_json::from_slice(envelope) {
        Ok(value) => value,
        Err(_) => return malformed(),
    };
    let sealed_bytes: Vec<u8> = match value.get("sealed").and_then(|v| v.as_array()) {
        Some(array) => match array
            .iter()
            .map(|n| n.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect::<Option<Vec<u8>>>()
        {
            Some(bytes) => bytes,
            None => return malformed(),
        },
        None => return malformed(),
    };
    let entries: BTreeMap<String, Vec<u8>> = match serde_json::from_slice(&sealed_bytes) {
        Ok(entries) => entries,
        Err(_) => return malformed(),
    };
    let mut names: Vec<String> = entries
        .keys()
        .filter(|name| name.starts_with("model/"))
        .filter_map(|name| name.split('/').nth(2).map(str::to_string))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Builds the Vault and the Model Registry for `assemble` (ADR 0020/0021).
///
/// 1. Vault: from the env Vault key when one is configured — reopening the
///    sealed envelope when the store has one (a failed unseal is a startup
///    error, never a half-filled Vault) — else a random per-process key
///    (the old behaviour).
/// 2. No Vault key + a sealed envelope: degraded mode. The Vault stays
///    empty of model keys, `vault_locked` is true, and the account names
///    visible in the envelope are reported as locked; a warning is logged
///    (no key material). Env keys still work in this mode.
/// 3. Env accounts are `put` into the Vault under `model/<kind>/<account>`
///    and registered in the Registry. A default model may be empty
///    (ADR 0020: the model may be set per Role in the chain; `resolve`
///    uses the chain's explicit model, never the default).
/// 4. Accounts from the `model_accounts` table are registered from their
///    stored metadata; one whose key is not in the Vault (for example in
///    degraded mode) stays registered but is recorded as locked.
///
/// No network call happens anywhere here. Nothing is wired to a Role: the
/// Registry is built and stored in `App`, and model chains stay the user's
/// own wiring (ADR 0020/0023).
///
/// Public within the crate so tests can drive the Vault/Registry path
/// against a store they seeded, without going through `assemble`.
pub(crate) async fn build_vault_and_registry(
    config: &Config,
    store: &SqlStore,
) -> Result<(Vault, Registry, bool, Vec<String>), Error> {
    let sealed = store.load_sealed_vault().await?;
    let (vault, vault_locked, mut locked_accounts) = match (config.vault_key.clone(), sealed) {
        (Some(key), Some(bytes)) => (Vault::unseal_envelope(key, &bytes)?, false, Vec::new()),
        (Some(key), None) => (Vault::new(key), false, Vec::new()),
        (None, Some(bytes)) => {
            let locked = locked_account_names_from_envelope(&bytes);
            eprintln!(
                "basileia: WARNING: no BASILEIA_VAULT_KEY configured, but a sealed vault \
                 exists: {} persisted model account(s) locked ({}); restart with the \
                 vault key to unlock them",
                locked.len(),
                locked.join(", ")
            );
            let mut key = [0u8; 32];
            getrandom::fill(&mut key)?;
            (Vault::new(key.into()), true, locked)
        }
        (None, None) => {
            let mut key = [0u8; 32];
            getrandom::fill(&mut key)?;
            (Vault::new(key.into()), false, Vec::new())
        }
    };

    let mut registry = Registry::new();

    // Env provisioning (ADR 0020 §Decision 4): keys go straight into the
    // Vault and the env strings die with `config` — nothing here copies a
    // plaintext key anywhere else.
    for account in &config.model_accounts {
        let secret = account.vault_secret();
        if !account.key.is_empty() {
            vault.put(&secret, &account.key);
        }
        registry.add_account(ModelAccount::new(
            &account.name,
            account.kind,
            &account.base_url,
            &account.default_model,
        ))?;
    }

    // Accounts added through the Agora (ADR 0023): metadata from the table,
    // key from the Vault when it is there.
    for record in store.load_model_accounts().await? {
        let kind = ModelKind::from_str(&record.kind).ok_or_else(|| {
            StoreError::Invalid(format!(
                "stored model account `{}` has unknown kind `{}`",
                record.name, record.kind
            ))
        })?;
        registry.add_account(ModelAccount::new(
            &record.name,
            kind,
            &record.base_url,
            &record.default_model,
        ))?;
        if kind != ModelKind::Ollama
            && !vault
                .names()
                .contains(&ModelAccount::vault_secret_name(kind, &record.name))
        {
            locked_accounts.push(record.name.clone());
        }
    }
    locked_accounts.sort();
    locked_accounts.dedup();

    Ok((vault, registry, vault_locked, locked_accounts))
}

/// Builds the whole application WITHOUT binding a socket.
///
/// Fails (rather than starting an open or half-built server) when the admin
/// password is missing/empty, the database cannot be opened, or the Agora
/// cannot be built.
pub async fn assemble(config: Config) -> Result<App, Error> {
    if config.admin_user.trim().is_empty() {
        return Err(crate::config::ConfigError::EmptyUser.into());
    }
    if config.admin_password.is_empty() {
        return Err(crate::config::ConfigError::MissingPassword.into());
    }

    let store = match &config.db {
        DbConfig::Memory => SqlStore::sqlite_in_memory().await?,
        DbConfig::File(path) => SqlStore::sqlite_file(path).await?,
    };

    let roster = build_roster(&store).await?;
    let graph = Arc::new(build_graph()?);

    // The runner: for each Job the Agora accepts, run its Graph durably
    // against the SqlStore, with the Agora as approver, pumping events live
    // to the Agora so they appear in the console.
    //
    // The builder takes a plain closure returning `BoxFuture<'static, ()>`
    // (`AgoraBuilder::runner`, `archodin-console/src/lib.rs:334`); the
    // closure is wrapped in an `Arc<Runner>` by the builder itself. The
    // future is `'static` because everything it borrows (`graph`, `store`,
    // `ctx`) is owned by the `async move` block.
    let runner_graph = Arc::clone(&graph);
    let runner_store = store.clone();
    let runner = move |ctx: JobContext| {
        let graph = Arc::clone(&runner_graph);
        let store = runner_store.clone();
        Box::pin(async move {
            let initial = CompanyState::from_request(&ctx.request.request);
            let events = graph.stream_job(&ctx.job, initial, &store, &ctx.agora);
            pump(events, &ctx.sink).await;
        }) as BoxFuture<'static, ()>
    };

    // The Vault and the Model Registry (ADR 0020/0021): the Vault key comes
    // from the env when configured (reopening the sealed envelope), else a
    // random per-process key; env model keys are provisioned into the Vault
    // and registered, and table accounts are registered from their stored
    // metadata. No model chain is wired to any Role here — the Registry is
    // built and stored in `App`, and wiring is the user's own work.
    let (vault, registry, vault_locked, locked_accounts) =
        build_vault_and_registry(&config, &store).await?;

    // Auth is mandatory: the password is hashed at startup and never stored.
    // `hash_password(&str) -> String` (plain `String`, NOT a `Result` —
    // `archodin-console/src/auth.rs:31`); the only `Auth` constructor is
    // `Auth::from_vault` (`auth.rs:72`), which reads the username and the
    // argon2 hash out of a `Vault`.
    //
    // The Agora login goes into the SAME Vault that holds the model keys,
    // under its pre-existing `agora/*` names — so `Auth::from_vault` keeps
    // working unchanged. A restart re-hashes the password from the
    // environment into the Vault; no fixed key is ever baked in (in
    // particular, never the `[9; 32]` test key from the KR-07 branch's
    // tests). When the Vault came from the sealed envelope, any `agora/*`
    // secret restored from it is overwritten here with the fresh hash.
    vault.put("agora/username", &config.admin_user);
    vault.put(
        "agora/password_hash",
        &hash_password(&config.admin_password),
    );
    // Plain HTTP by design: this server has no TLS, so a `Secure` cookie
    // would never be sent back by a browser over a LAN address and login
    // would break as soon as `BASILEIA_BIND=0.0.0.0` is used. `Secure`
    // without TLS buys no real protection, so it is switched off
    // explicitly (default is `true`, `auth.rs:106`) — see the crate README.
    // Revisit if TLS is ever terminated in front of this server.
    let auth =
        Auth::from_vault(&vault, "agora/username", "agora/password_hash")?.secure_cookie(false);
    // The Model API (ADR 0020/0024): the same Vault, the same Registry
    // (shared behind one `Arc<Mutex<..>>`, so provisioning through the
    // API and reads here never drift), the same store, and the same
    // UsageMeter the Agora shows — one meter for job usage and model
    // budgets. `vault_key_configured` tells the API whether mutations
    // can be persisted (without a Vault key they apply in-memory only).
    // The Vault is cloned (cheap: one shared `Arc` inside), so the API
    // and this `App` hold the single source of truth.
    let registry = Arc::new(Mutex::new(registry));
    let meter = Arc::new(UsageMeter::new());
    let model_api = ModelApi::new(
        vault.clone(),
        Arc::clone(&registry),
        Arc::new(store.clone()) as Arc<dyn ModelStore + Send + Sync>,
        Arc::clone(&meter),
        config.vault_key.is_some(),
        vault_locked,
    );
    // `Agora::builder()` + `.auth(...)` + `.runner(...)` + `.build() ->
    // Agora` and `.router(&self) -> Router` — all verified against
    // `archodin-console/src/lib.rs` (`:445`, `:373`, `:334`, `:403`,
    // `:764`). On the KR-07 branch `build()` additionally panics without
    // `.auth(...)`; we always pass it, so nothing changes here.
    let agora = Agora::builder()
        .auth(auth)
        .runner(runner)
        .meter(meter)
        .model_api(model_api)
        .build();
    let router = agora.router();

    Ok(App {
        agora,
        router,
        store,
        roster,
        vault,
        registry,
        vault_locked,
        locked_accounts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelAccountConfig;
    use archodin_store::{ModelAccountRecord, ModelAccountStore};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;
    use zeroize::Zeroizing;

    /// `sk-canary-*` placeholders only — never a real key (ADR 0021).
    const CANARY: &str = "sk-canary-9f3a2b";

    fn test_config() -> Config {
        Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            db: DbConfig::Memory,
            admin_user: "admin".to_string(),
            admin_password: "test-password-change-me".to_string(),
            vault_key: None,
            model_accounts: Vec::new(),
        }
    }

    fn env_account(name: &str, model: &str) -> ModelAccountConfig {
        ModelAccountConfig {
            name: name.to_string(),
            kind: ModelKind::OpenAiCompatible,
            base_url: "https://api.example.com/v1".to_string(),
            default_model: model.to_string(),
            key: CANARY.to_string(),
        }
    }

    /// A sealed envelope holding one model secret, as an earlier run with
    /// a Vault key would have persisted it.
    fn sealed_with_model_key(vault_key: [u8; 32]) -> Vec<u8> {
        let vault = Vault::new(vault_key.into());
        vault.put("model/openai_compatible/main", CANARY);
        vault.seal_envelope().unwrap()
    }

    #[tokio::test]
    async fn env_provisioning_reaches_the_vault_and_the_registry() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        let mut config = test_config();
        config.model_accounts = vec![env_account("main", "m1")];
        let (vault, registry, vault_locked, locked) =
            build_vault_and_registry(&config, &store).await.unwrap();
        assert!(!vault_locked);
        assert!(locked.is_empty());
        assert_eq!(
            vault
                .resolve("model/openai_compatible/main")
                .unwrap()
                .expose(),
            CANARY
        );
        let account = registry.account("main").expect("env account registered");
        assert_eq!(account.kind, ModelKind::OpenAiCompatible);
        assert_eq!(account.base_url, "https://api.example.com/v1");
        assert_eq!(account.default_model, "m1");
    }

    #[tokio::test]
    async fn no_vault_key_with_a_sealed_blob_is_degraded() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        store
            .save_sealed_vault(&sealed_with_model_key([7; 32]))
            .await
            .unwrap();
        let (vault, registry, vault_locked, locked) =
            build_vault_and_registry(&test_config(), &store)
                .await
                .unwrap();
        assert!(vault_locked);
        assert_eq!(locked, vec!["main".to_string()]);
        // The fresh random Vault cannot open the sealed secret.
        assert!(vault.resolve("model/openai_compatible/main").is_err());
        assert!(registry.account("main").is_none());
    }

    #[tokio::test]
    async fn the_vault_key_reopens_the_sealed_blob() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        store
            .save_sealed_vault(&sealed_with_model_key([7; 32]))
            .await
            .unwrap();
        let mut config = test_config();
        config.vault_key = Some(Zeroizing::new([7; 32]));
        let (vault, _, vault_locked, locked) =
            build_vault_and_registry(&config, &store).await.unwrap();
        assert!(!vault_locked);
        assert!(locked.is_empty());
        assert_eq!(
            vault
                .resolve("model/openai_compatible/main")
                .unwrap()
                .expose(),
            CANARY
        );
    }

    #[tokio::test]
    async fn a_wrong_vault_key_is_a_startup_error_not_a_half_filled_vault() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        store
            .save_sealed_vault(&sealed_with_model_key([7; 32]))
            .await
            .unwrap();
        let mut config = test_config();
        config.vault_key = Some(Zeroizing::new([8; 32]));
        assert!(
            build_vault_and_registry(&config, &store).await.is_err(),
            "a wrong Vault key must fail startup, never start half-filled"
        );
    }

    #[tokio::test]
    async fn a_malformed_envelope_degrades_with_an_empty_lock_list() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        // A versioned envelope whose `sealed` bytes are not a vault map:
        // `save_sealed_vault` accepts the shape, the restore must not panic.
        store
            .save_sealed_vault(b"{\"version\":1,\"sealed\":[1,2,3]}")
            .await
            .unwrap();
        let (_, _, vault_locked, locked) = build_vault_and_registry(&test_config(), &store)
            .await
            .unwrap();
        assert!(vault_locked);
        assert!(locked.is_empty());
    }

    #[tokio::test]
    async fn a_table_account_without_a_key_is_registered_but_locked() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        store
            .save_model_account(&ModelAccountRecord {
                name: "work".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                default_model: "m1".to_string(),
                budget_tokens: None,
                budget_cost: None,
            })
            .await
            .unwrap();
        let (_, registry, vault_locked, locked) = build_vault_and_registry(&test_config(), &store)
            .await
            .unwrap();
        assert!(!vault_locked);
        assert!(registry.account("work").is_some());
        assert_eq!(locked, vec!["work".to_string()]);
    }

    #[tokio::test]
    async fn a_table_account_with_an_env_key_is_not_locked() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        store
            .save_model_account(&ModelAccountRecord {
                name: "main".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                default_model: "m1".to_string(),
                budget_tokens: None,
                budget_cost: None,
            })
            .await
            .unwrap();
        let mut config = test_config();
        config.model_accounts = vec![env_account("main", "m1")];
        let (_, registry, vault_locked, locked) =
            build_vault_and_registry(&config, &store).await.unwrap();
        assert!(!vault_locked);
        assert!(locked.is_empty());
        assert!(registry.account("main").is_some());
    }

    #[tokio::test]
    async fn an_env_account_without_a_model_is_registered_with_an_empty_default() {
        let store = SqlStore::sqlite_in_memory().await.unwrap();
        let mut config = test_config();
        config.model_accounts = vec![env_account("main", "")];
        let (vault, registry, _, locked) = build_vault_and_registry(&config, &store).await.unwrap();
        assert!(locked.is_empty());
        assert_eq!(
            vault
                .resolve("model/openai_compatible/main")
                .unwrap()
                .expose(),
            CANARY
        );
        assert!(
            registry.account("main").is_some(),
            "an empty default model does not block registration (ADR 0020); the model comes from the chain"
        );
        assert_eq!(registry.account("main").unwrap().default_model, "");
    }

    #[tokio::test]
    async fn assemble_stores_the_vault_and_registry_in_app() {
        let mut config = test_config();
        config.model_accounts = vec![env_account("main", "m1")];
        let app = assemble(config).await.unwrap();
        assert!(!app.vault_locked);
        assert!(app.locked_accounts.is_empty());
        assert_eq!(
            app.vault
                .resolve("model/openai_compatible/main")
                .unwrap()
                .expose(),
            CANARY
        );
        assert!(app.registry.lock().unwrap().account("main").is_some());
        // The Agora login still lands in the same Vault.
        assert!(app.vault.resolve("agora/username").is_ok());
    }

    #[tokio::test]
    async fn assemble_wires_the_model_api_into_the_router() {
        let mut config = test_config();
        config.admin_password = "test-admin-password".to_string();
        config.model_accounts = vec![env_account("main", "m1")];
        let app = assemble(config).await.unwrap();
        // Without a session the route exists and is protected: 401, not 404.
        let res = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/models/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // Log in with the test admin, then the status endpoint answers.
        let login = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"username": "admin", "password": "test-admin-password"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get("set-cookie")
            .expect("login sets a session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .expect("cookie value")
            .to_string();
        let res = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/models/status")
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
