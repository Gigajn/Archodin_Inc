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

use std::sync::Arc;

use archodin_console::{hash_password, pump, Agora, Auth, JobContext};
use archodin_graph::{Graph, Role, Roster, RosterError, Vault};
use archodin_store::{restore_roster, save_roster, RosterStore, SqlStore, StoreError};
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

    // Auth is mandatory: the password is hashed at startup and never stored.
    // `hash_password(&str) -> String` (plain `String`, NOT a `Result` —
    // `archodin-console/src/auth.rs:31`); the only `Auth` constructor is
    // `Auth::from_vault` (`auth.rs:72`), which reads the username and the
    // argon2 hash out of a `Vault`.
    //
    // The Vault key is random per process (`getrandom`), generated here and
    // never persisted, logged, or reused across restarts: the Vault only
    // carries the credentials from startup to `from_vault`, and a restart
    // re-hashes the password from the environment into a fresh Vault. No
    // fixed key is ever baked in (in particular, never the `[9; 32]` test
    // key from the KR-07 branch's tests). The key's stack copy is dropped
    // at the end of this block; the cipher inside the Vault keeps its own.
    let vault = {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key)?;
        Vault::new(key)
    };
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
    // `Agora::builder()` + `.auth(...)` + `.runner(...)` + `.build() ->
    // Agora` and `.router(&self) -> Router` — all verified against
    // `archodin-console/src/lib.rs` (`:445`, `:373`, `:334`, `:403`,
    // `:764`). On the KR-07 branch `build()` additionally panics without
    // `.auth(...)`; we always pass it, so nothing changes here.
    let agora = Agora::builder().auth(auth).runner(runner).build();
    let router = agora.router();

    Ok(App {
        agora,
        router,
        store,
        roster,
    })
}
