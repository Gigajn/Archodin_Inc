#![allow(dead_code)]
//! One behaviour suite, run against every backend: in-memory, SQLite and (when
//! Docker is available, with `--ignored`) Postgres. Each test asks its backend
//! to `open` the storage again after "a restart" and expects the company intact.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use archodin_graph::{
    Akasha, ApprovalDecision, Approver, CheckpointStore, Graph, JobOutcome, Role, Roster,
    ScriptedApprover, Skill, SkillLibrary,
};
use archodin_store::{
    load_akasha, load_library, restore_roster, save_akasha, save_library, save_roster, AkashaStore,
    RosterStore, SkillStore,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub total: u32,
}

/// Everything a backend must provide.
pub trait Company:
    CheckpointStore<Tally> + RosterStore + SkillStore + AkashaStore + 'static
{
}
impl<T: CheckpointStore<Tally> + RosterStore + SkillStore + AkashaStore + 'static> Company for T {}

/// Opens the storage; calling it again is "the process restarted".
pub type Open<T> = Box<dyn Fn() -> Pin<Box<dyn Future<Output = Arc<T>>>>>;

pub fn opener<T, F, Fut>(f: F) -> Open<T>
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Arc<T>> + 'static,
{
    Box::new(move || Box::pin(f()))
}

fn skill(name: &str, body: &str) -> Skill {
    Skill::from_markdown(&format!(
        "---\nname: {name}\ndescription: does {name}\nkind: flow\nuses: tidy, check\n---\n{body}\n"
    ))
    .unwrap()
    .script("run.sh", "echo hi")
}

pub async fn approve_all(library: &SkillLibrary) {
    let approver = ScriptedApprover::new(std::iter::repeat_n(ApprovalDecision::Approved, 10));
    library.review_pending(&approver as &dyn Approver).await;
}

pub async fn job_interrupted_by_restart_resumes_from_its_checkpoint<T: Company>(open: Open<T>) {
    let first_runs = Arc::new(AtomicUsize::new(0));
    let graph = |counter: Arc<AtomicUsize>| -> Graph<Tally> {
        Graph::builder()
            .node("first", move |s: Tally| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Tally { total: s.total + 1 }
                }
            })
            .node_with_approval(
                "second",
                |s: &Tally| format!("add 10 to {}?", s.total),
                |s: Tally| async move {
                    Tally {
                        total: s.total + 10,
                    }
                },
            )
            .edge("first", "second")
            .entry("first")
            .build()
            .unwrap()
    };

    let store = open().await;
    let paused = graph(first_runs.clone())
        .run_job(
            "job-1",
            Tally { total: 0 },
            &*store,
            &ScriptedApprover::new([]),
        )
        .await
        .unwrap();
    assert!(matches!(paused, JobOutcome::Paused { .. }));
    drop(store);

    // The process restarts: a new storage handle, a new graph, a new approver.
    let store = open().await;
    let saved = store.checkpoints("job-1").await.unwrap();
    assert_eq!(saved.iter().map(|c| c.step).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(saved[1].next.as_deref(), Some("second"));
    let done = graph(first_runs.clone())
        .run_job(
            "job-1",
            Tally { total: 0 },
            &*store,
            &ScriptedApprover::new([ApprovalDecision::Approved]),
        )
        .await
        .unwrap();
    assert_eq!(done, JobOutcome::Finished(Tally { total: 11 }));
    assert_eq!(
        first_runs.load(Ordering::SeqCst),
        1,
        "the first step ran again"
    );
    let all = store.checkpoints("job-1").await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].next, None);
    assert!(store.checkpoints("other-job").await.unwrap().is_empty());
}

pub async fn agents_keep_names_roles_and_order_across_restarts<T: Company>(open: Open<T>) {
    let roster = Roster::new();
    roster.add_role(
        Role::new("Engineer", "Write code")
            .skill("review")
            .budget_tokens(5000),
    );
    roster.add_role(Role::new("Writer", "Write docs").without_budget_limit());
    roster.hire("Engineer", "Ada").unwrap();
    roster.hire("Writer", "Bo").unwrap();
    roster.hire("Engineer", "Cy").unwrap();
    save_roster(&*open().await, &roster).await.unwrap();

    let restored = restore_roster(&*open().await, |role| role).await.unwrap();
    let names: Vec<_> = restored.agents().iter().map(|a| a.title()).collect();
    assert_eq!(
        names,
        vec!["Ada (Engineer)", "Bo (Writer)", "Cy (Engineer)"]
    );
    let engineer = restored.role("Engineer").unwrap();
    assert_eq!(engineer.instructions(), "Write code");
    assert_eq!(engineer.skills(), ["review"]);
    assert_eq!(engineer.budget(), Some(5000));
    assert_eq!(restored.role("Writer").unwrap().budget(), None);

    // Releasing an Agent and saving again is remembered too.
    restored.release("Bo");
    restored.hire("Writer", "Di").unwrap();
    save_roster(&*open().await, &restored).await.unwrap();
    let again = restore_roster(&*open().await, |role| role).await.unwrap();
    let names: Vec<_> = again
        .agents()
        .iter()
        .map(|a| a.name().to_string())
        .collect();
    assert_eq!(names, vec!["Ada", "Cy", "Di"]);
}

pub async fn restoring_lets_the_caller_add_back_what_is_code<T: Company>(open: Open<T>) {
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Write code"));
    roster.hire("Engineer", "Ada").unwrap();
    save_roster(&*open().await, &roster).await.unwrap();

    let restored = restore_roster(&*open().await, |role| role.skill("added-in-code"))
        .await
        .unwrap();
    assert_eq!(
        restored.role("Engineer").unwrap().skills(),
        ["added-in-code"]
    );
    assert!(restored.agent("Ada").is_some());
}

pub async fn skills_keep_versions_sharing_and_script_approval<T: Company>(open: Open<T>) {
    let library = SkillLibrary::new();
    library.propose("Engineer", skill("deploy", "v1"));
    approve_all(&library).await;
    library.propose("Engineer", skill("deploy", "v2"));
    approve_all(&library).await;
    library
        .review_scripts(
            "Engineer",
            "deploy",
            &ScriptedApprover::new([ApprovalDecision::Approved]),
        )
        .await
        .unwrap();
    library.share("Engineer", "deploy").unwrap();
    library.propose("Writer", skill("waiting", "not yet"));
    save_library(&*open().await, &library).await.unwrap();

    let restored = load_library(&*open().await).await.unwrap();
    assert_eq!(restored.versions("Engineer", "deploy"), vec![1, 2]);
    assert_eq!(restored.pending().len(), 1);
    assert_eq!(restored.pending()[0].skill.name(), "waiting");
    // Shared: another Role sees it, with its body, uses and scripts intact.
    let seen = restored.skills_for_role("Writer");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body(), "v2\n");
    assert_eq!(seen[0].uses(), ["tidy", "check"]);
    assert_eq!(
        restored
            .authorized_script("Engineer", "deploy", "run.sh")
            .unwrap(),
        "echo hi"
    );
    // Rollback still works and version numbers are not reused.
    restored.rollback("Engineer", "deploy").unwrap();
    assert_eq!(restored.skills_for_role("Engineer")[0].body(), "v1\n");
    save_library(&*open().await, &restored).await.unwrap();
    let again = load_library(&*open().await).await.unwrap();
    assert_eq!(again.skills_for_role("Engineer")[0].body(), "v1\n");
    again.propose("Engineer", skill("deploy", "v3"));
    approve_all(&again).await;
    assert_eq!(again.versions("Engineer", "deploy"), vec![1, 2, 3]);
}

pub async fn akasha_facts_survive_and_stay_free_of_duplicates<T: Company>(open: Open<T>) {
    let store = open().await;
    assert!(store
        .remember("Engineer", "Acme pays on the 5th.")
        .await
        .unwrap());
    assert!(!store
        .remember("Engineer", "acme  pays on the 5th")
        .await
        .unwrap());
    assert!(store
        .remember("Engineer", "Bob owns billing")
        .await
        .unwrap());
    assert!(!store.remember("Engineer", "  ").await.unwrap());
    assert!(store
        .remember("Writer", "Acme pays on the 5th.")
        .await
        .unwrap());
    drop(store);

    let store = open().await;
    assert_eq!(
        store.facts("Engineer").await.unwrap(),
        vec!["Acme pays on the 5th.", "Bob owns billing"]
    );
    assert_eq!(
        store.fact_roles().await.unwrap(),
        vec!["Engineer", "Writer"]
    );
    assert!(store.facts("Nobody").await.unwrap().is_empty());
}

pub async fn an_akasha_can_be_saved_and_loaded<T: Company>(open: Open<T>) {
    let akasha = Akasha::new();
    akasha.remember_for_role("Engineer", "Use tabs");
    akasha.remember_for_role("Writer", "Be brief");
    save_akasha(&*open().await, &akasha).await.unwrap();
    save_akasha(&*open().await, &akasha).await.unwrap(); // saving twice adds nothing
    let loaded = load_akasha(&*open().await).await.unwrap();
    assert_eq!(loaded.facts("Engineer"), vec!["Use tabs"]);
    assert_eq!(loaded.facts("Writer"), vec!["Be brief"]);
}

macro_rules! behaviour {
    ($open:expr, [$($attr:meta),*], $name:ident) => {
        #[tokio::test(flavor = "multi_thread")]
        $(#[$attr])*
        async fn $name() {
            crate::common::$name($open).await;
        }
    };
}

/// Runs every behaviour against the backend whose `Open` the expression builds.
macro_rules! suite {
    ($open:expr, [$($attr:meta),*]) => {
        behaviour!($open, [$($attr),*], job_interrupted_by_restart_resumes_from_its_checkpoint);
        behaviour!($open, [$($attr),*], agents_keep_names_roles_and_order_across_restarts);
        behaviour!($open, [$($attr),*], restoring_lets_the_caller_add_back_what_is_code);
        behaviour!($open, [$($attr),*], skills_keep_versions_sharing_and_script_approval);
        behaviour!($open, [$($attr),*], akasha_facts_survive_and_stay_free_of_duplicates);
        behaviour!($open, [$($attr),*], an_akasha_can_be_saved_and_loaded);
    };
}
