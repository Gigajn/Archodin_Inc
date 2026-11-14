//! Smoke tests for the Basileia assembly.
//!
//! TDD: this file was written BEFORE `src/assembly.rs` existed, so it fails
//! until the assembly is implemented, then passes.
//!
//! The tests build the whole application state — SqlStore, Roster, Graph,
//! Agora with auth, router — WITHOUT binding any network socket. They prove
//! the assembly holds together; they do not start the server (serving is
//! only done by `main`).
//!
//! NOTE: these tests have NOT been executed yet — there is no local cargo
//! toolchain in this environment and the GitHub connector was rate-limited
//! (HTTP 429) before the branch could be created. Run `cargo test -p basileia`
//! locally before merging.

use archodin_graph::{ApprovalDecision, JobOutcome, ScriptedApprover};
use archodin_store::RosterStore;
use basileia::assembly::build_graph;
use basileia::{assemble, CompanyState, Config, ConfigError, DbConfig};

fn test_config() -> Config {
    Config {
        bind: "127.0.0.1:0".parse().expect("test bind address parses"),
        db: DbConfig::Memory,
        admin_user: "admin".to_string(),
        admin_password: "test-password-change-me".to_string(),
        vault_key: None,
        model_accounts: Vec::new(),
    }
}

/// The assembly builds the router and the persisted roster without panic
/// and without touching the network.
#[tokio::test]
async fn assemble_builds_router_without_binding_network() {
    let app = assemble(test_config())
        .await
        .expect("assembly succeeds with an in-memory store and test credentials");

    // The roster was seeded AND persisted through the SqlStore: the Graph,
    // Roster and Store are really wired, not just constructed.
    let roles = app
        .store
        .roles()
        .await
        .expect("seeded roles read back from the store");
    assert!(
        !roles.is_empty(),
        "seeded roles must be persisted in the store"
    );
    assert!(
        roles.iter().any(|r| r.name == "Strategos"),
        "the Strategos role must be among the seeded roles"
    );

    // The router exists; dropping it here proves nothing was bound — the
    // server only starts when `main` serves it.
    drop(app.router);
}

/// Auth is mandatory (KR-07): the application refuses to assemble — and
/// therefore refuses to start — when no admin password is configured.
#[tokio::test]
async fn assemble_refuses_to_start_without_admin_password() {
    let mut config = test_config();
    config.admin_password = String::new();
    assert!(
        assemble(config).await.is_err(),
        "an empty admin password must fail assembly, never start an open server"
    );
}

/// Restores the Basileia environment variables it changed when dropped, so
/// this test never leaks configuration into other tests in the same process.
struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn apply(vars: &[(&'static str, Option<&str>)]) -> Self {
        let saved = vars
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect();
        for (key, value) in vars {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// `Config::from_env` reads every variable, and a missing or empty admin
/// password is refused (never silently defaulted).
#[test]
fn config_from_env_reads_all_vars_and_requires_a_password() {
    let _guard = EnvGuard::apply(&[
        ("BASILEIA_BIND", Some("127.0.0.1:9090")),
        ("BASILEIA_DB", Some(":memory:")),
        ("BASILEIA_ADMIN_USER", Some("boss")),
        ("BASILEIA_ADMIN_PASSWORD", Some("s3cret")),
    ]);
    let config = Config::from_env().expect("all four vars are set and valid");
    assert_eq!(config.bind, "127.0.0.1:9090".parse().unwrap());
    assert_eq!(config.db, DbConfig::Memory);
    assert_eq!(config.admin_user, "boss");
    assert_eq!(config.admin_password, "s3cret");

    std::env::remove_var("BASILEIA_ADMIN_PASSWORD");
    assert!(
        matches!(Config::from_env(), Err(ConfigError::MissingPassword)),
        "a missing admin password must be refused, not defaulted"
    );

    std::env::set_var("BASILEIA_ADMIN_PASSWORD", "");
    assert!(
        matches!(Config::from_env(), Err(ConfigError::MissingPassword)),
        "an empty admin password must be refused, not defaulted"
    );

    std::env::set_var("BASILEIA_ADMIN_PASSWORD", "s3cret");
    std::env::set_var("BASILEIA_BIND", "not-an-address");
    assert!(
        matches!(Config::from_env(), Err(ConfigError::BadBind(..))),
        "an unparsable bind address must be reported, not ignored"
    );
}

/// The approval path is real end-to-end: a Job pauses at `acknowledge`
/// while the approver has no answer, and the same Job id resumes from its
/// checkpoint and finishes once the approver says yes. The Graph is the
/// one `assemble` wires (`build_graph`); the store is the assembled app's
/// SqlStore, so the checkpoint resume crosses the real storage layer.
/// (`ScriptedApprover` stands in for the human at the Agora console — the
/// Agora itself is the approver in production.)
#[tokio::test]
async fn approval_round_trip_pauses_then_finishes() {
    let app = assemble(test_config())
        .await
        .expect("assembly succeeds with an in-memory store and test credentials");
    let graph = build_graph().expect("the job machine builds");
    let initial = CompanyState {
        request: "hello".to_string(),
        notes: Vec::new(),
        outcome: None,
    };

    // Nobody answers: the Job pauses at `acknowledge`, nothing more runs.
    let undecided = ScriptedApprover::new(Vec::<ApprovalDecision>::new());
    let outcome = graph
        .run_job("job-1", initial.clone(), &app.store, &undecided)
        .await
        .expect("a paused Job is an outcome, not an error");
    match &outcome {
        JobOutcome::Paused { node, prompt, .. } => {
            assert_eq!(node, "acknowledge");
            assert!(
                prompt.contains("hello"),
                "the approval prompt names the request: {prompt}"
            );
        }
        other => panic!("expected the Job to pause at acknowledge, got {other:?}"),
    }
    assert_eq!(
        undecided.requests().len(),
        1,
        "exactly one approval was asked"
    );

    // Same Job id, same store: resumes from the checkpoint past `intake`
    // and, once approved, runs `acknowledge` and `finish` to completion.
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let outcome = graph
        .run_job("job-1", initial, &app.store, &approver)
        .await
        .expect("an approved Job runs to completion");
    match outcome {
        JobOutcome::Finished(state) => {
            assert_eq!(state.outcome.as_deref(), Some("done: hello"));
            assert!(
                state.notes.iter().any(|n| n.contains("intake")),
                "the resumed Job kept the intake step: {notes:?}",
                notes = state.notes
            );
            assert!(
                state.notes.iter().any(|n| n.contains("acknowledge")),
                "the approved step ran: {notes:?}",
                notes = state.notes
            );
        }
        other => panic!("expected the Job to finish after approval, got {other:?}"),
    }
    assert_eq!(
        approver.requests().len(),
        1,
        "the resumed Job asks once more"
    );
}
