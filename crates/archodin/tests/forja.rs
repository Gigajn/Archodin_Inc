use std::sync::{Arc, Mutex};

use archodin::graph::{
    ApprovalDecision, Budget, Explorador, Forja, ForjaError, ForjaGitHub, ForjaWork, GitHub,
    GitHubCall, GitHubError, Issue, JobQueue, Nakama, OpenPullRequest, RepoInfo, RepoRef, Role,
    Roster, Sandbox, SandboxError, SandboxOutput, ScriptedApprover, ScriptedGitHub,
    ScriptedSandbox, Usage, UsageMeter,
};
use futures::future::BoxFuture;

fn home() -> RepoRef {
    RepoRef::new("Gigajn", "Archodin_Inc")
}

fn scripted() -> ScriptedGitHub {
    ScriptedGitHub::new("Gigajn")
        .repository(RepoInfo {
            repo: home(),
            description: Some("Archodin".into()),
            default_branch: "main".into(),
        })
        .repository(RepoInfo {
            repo: RepoRef::new("acme", "widgets"),
            description: None,
            default_branch: "main".into(),
        })
        .issue(ticket())
        .issue(Issue::new(40, "Not ready", "", "u/40"))
}

fn fake() -> Arc<ScriptedGitHub> {
    Arc::new(scripted())
}

fn guarded(fake: &Arc<ScriptedGitHub>) -> ForjaGitHub<Arc<ScriptedGitHub>> {
    ForjaGitHub::new(fake.clone(), home())
}

fn ticket() -> Issue {
    Issue::new(
        31,
        "Forja",
        "Build it",
        "https://github.com/Gigajn/Archodin_Inc/issues/31",
    )
}

fn ok(code: i32) -> Result<SandboxOutput, SandboxError> {
    Ok(SandboxOutput {
        stdout: "out".into(),
        stderr: "err".into(),
        exit_code: code,
    })
}

async fn forja_of(
    meter: &UsageMeter,
    agent: &'static str,
    repo: &str,
) -> Result<Forja, ForjaError> {
    let roster = Roster::new();
    roster.add_role(Role::new("Contributor", "Build Archodin"));
    let nakama = Nakama::new([agent], |_: &str| Role::new("X", "x"));
    let explorador = Explorador::new(["devtools"]);
    let charter = explorador
        .charter_from_link(&scripted(), repo)
        .await
        .unwrap()
        .with_budget(Budget::tokens(100));
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let department = explorador
        .approve(charter, &approver, &roster, &nakama, meter)
        .await
        .unwrap()
        .unwrap();
    Forja::new(department, home())
}

async fn forja(meter: &UsageMeter) -> Forja {
    forja_of(meter, "Ada", "Gigajn/Archodin_Inc").await.unwrap()
}

fn work() -> ForjaWork {
    ForjaWork::new("forja department", "Add Forja", "Adds Forja.")
        .check("cargo test")
        .file("crates/x/src/forja.rs", "// forja", "Add forja module")
}

#[tokio::test]
async fn forja_is_created_from_a_charter_that_points_at_archodin() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    assert_eq!(forja.department().agents(), ["Ada"]);
    assert_eq!(forja.department().charter().source(), Some(&home()));
}

#[tokio::test]
async fn forja_refuses_a_department_for_another_repository() {
    let meter = UsageMeter::new();
    let err = forja_of(&meter, "Ada", "acme/widgets").await.err().unwrap();
    assert!(matches!(err, ForjaError::WrongRepository { .. }));
}

#[tokio::test]
async fn only_ready_tickets_become_jobs_and_a_ticket_with_an_open_pull_request_is_skipped() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = Arc::new(scripted().open_pull_request_entry(OpenPullRequest {
        number: 3,
        head: "Bo/other".into(),
        body: "Closes #40\n".into(),
    }));
    let github = guarded(&fake);
    let queue = JobQueue::new();
    let ready = |i: &Issue| i.number == 31 || i.number == 40;
    let ids = forja
        .pick_up(&github, &queue, &meter, ready, |i| i.clone())
        .await
        .unwrap();
    assert_eq!(ids.len(), 2 - 1);
    assert_eq!(queue.next().unwrap().state.number, 31);
}

#[tokio::test]
async fn a_ticket_is_queued_again_until_its_build_succeeds() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = fake();
    let github = guarded(&fake);
    let queue = JobQueue::new();
    let pick = || forja.pick_up(&github, &queue, &meter, |i| i.number == 31, |i| i.clone());

    assert_eq!(pick().await.unwrap().len(), 1);
    // Queued and in flight: not queued twice.
    assert!(pick().await.unwrap().is_empty());

    // The build fails, so the ticket can be retried.
    let failing = ScriptedSandbox::new([ok(1)]);
    let err = forja
        .build(&failing, &github, &meter, "Ada", &ticket(), work())
        .await;
    assert!(err.is_err());
    assert_eq!(pick().await.unwrap().len(), 1);

    // The build succeeds, so the ticket is done.
    let passing = ScriptedSandbox::new([ok(0)]);
    forja
        .build(&passing, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap();
    assert!(pick().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_passing_build_opens_one_pull_request_that_links_the_ticket_and_says_ai() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = fake();
    let github = guarded(&fake);
    let sandbox = ScriptedSandbox::new([ok(0)]);

    let made = forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap();
    assert_eq!(made.branch, "Ada/forja-department");
    assert_eq!(made.pull_request.number, 7);
    assert_eq!(sandbox.commands(), ["cargo test"]);

    let calls = fake.calls();
    assert!(
        matches!(&calls[1], GitHubCall::CreateBranch { repo, branch, from }
        if *repo == home() && branch == "Ada/forja-department" && from == "main")
    );
    let GitHubCall::OpenPullRequest { upstream, draft } = calls.last().unwrap() else {
        panic!("last call opens the pull request");
    };
    assert_eq!(*upstream, home());
    assert_eq!(draft.base, "main");
    assert_eq!(draft.head, "Ada/forja-department");
    assert!(draft.body.contains("Closes #31"));
    assert!(draft.body.contains("issues/31"));
    assert!(draft.body.contains("made with AI"));
}

/// A Sandbox whose working directory really holds what was staged.
#[derive(Default)]
struct FileSandbox {
    files: Mutex<Vec<(String, String)>>,
}

impl Sandbox for FileSandbox {
    fn stage<'a>(
        &'a self,
        files: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<(), SandboxError>> {
        *self.files.lock().unwrap() = files.to_vec();
        Box::pin(async { Ok(()) })
    }

    /// `cat <path>`: prints the file, and fails when it is missing or says FAIL.
    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, Result<SandboxOutput, SandboxError>> {
        let path = command.trim_start_matches("cat ");
        let found = self
            .files
            .lock()
            .unwrap()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, c)| c.clone());
        Box::pin(async move {
            Ok(match found {
                Some(content) => SandboxOutput {
                    exit_code: i32::from(content.contains("FAIL")),
                    stdout: content,
                    stderr: "tests ran".into(),
                },
                None => SandboxOutput {
                    stdout: String::new(),
                    stderr: "no such file".into(),
                    exit_code: 1,
                },
            })
        })
    }
}

#[tokio::test]
async fn the_files_are_staged_into_the_sandbox_before_the_checks_run() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = fake();
    let github = guarded(&fake);

    let sandbox = FileSandbox::default();
    let passing = ForjaWork::new("t", "T", "S")
        .file("src/a.rs", "// fine", "Add a")
        .check("cat src/a.rs");
    forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), passing)
        .await
        .unwrap();

    // A check on a file that was never staged fails: nothing is tested by accident.
    let missing = ForjaWork::new("t2", "T", "S").check("cat src/none.rs");
    let err = forja
        .build(
            &FileSandbox::default(),
            &github,
            &meter,
            "Ada",
            &ticket(),
            missing,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::CheckFailed { .. }));

    // A check that fails on the staged content reports stdout and stderr.
    let failing = ForjaWork::new("t3", "T", "S")
        .file("src/b.rs", "// FAIL here", "Add b")
        .check("cat src/b.rs");
    let ForjaError::CheckFailed { output, .. } = forja
        .build(
            &FileSandbox::default(),
            &github,
            &meter,
            "Ada",
            &ticket(),
            failing,
        )
        .await
        .unwrap_err()
    else {
        panic!("check fails");
    };
    assert!(output.contains("// FAIL here") && output.contains("tests ran"));

    let scripted = ScriptedSandbox::new([ok(0)]);
    forja
        .build(&scripted, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap();
    assert_eq!(
        scripted.staged(),
        [("crates/x/src/forja.rs".to_string(), "// forja".to_string())]
    );
}

#[tokio::test]
async fn a_failing_check_opens_nothing() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = fake();
    let github = guarded(&fake);
    let sandbox = ScriptedSandbox::new([ok(101)]);
    let err = forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ForjaError::CheckFailed { exit_code: 101, .. }
    ));
    // Only reads: no branch, no commit, no pull request.
    assert!(fake
        .calls()
        .iter()
        .all(|c| matches!(c, GitHubCall::Repository(_))));

    let broken = ScriptedSandbox::new([Err(SandboxError::Runtime("boom".into()))]);
    let err = forja
        .build(&broken, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::Sandbox(_)));
}

#[tokio::test]
async fn only_the_departments_agents_build() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = guarded(&fake());
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let err = forja
        .build(&sandbox, &github, &meter, "Mallory", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::NotMember(_)));
    assert!(sandbox.commands().is_empty());
}

#[tokio::test]
async fn names_are_validated_before_anything_runs() {
    let meter = UsageMeter::new();
    let github = guarded(&fake());
    let sandbox = ScriptedSandbox::new([ok(0)]);

    let forja = forja(&meter).await;
    let empty_topic = ForjaWork::new("!!!", "T", "S").check("true");
    let err = forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), empty_topic)
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::InvalidName(_)));

    let spaced = self::forja_of(&meter, "Ada.Lovelace", "Gigajn/Archodin_Inc")
        .await
        .unwrap();
    let err = spaced
        .build(&sandbox, &github, &meter, "Ada.Lovelace", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::InvalidName(_)), "{err:?}");
    assert!(sandbox.commands().is_empty() && sandbox.staged().is_empty());
}

#[tokio::test]
async fn a_spent_budget_stops_a_build() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    meter.record(
        "job",
        &forja.department().budget_key(),
        Usage::new(90, 60, 0.0),
    );
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let err = forja
        .build(
            &sandbox,
            &guarded(&fake()),
            &meter,
            "Ada",
            &ticket(),
            work(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::BudgetExceeded));
    assert!(sandbox.commands().is_empty());
}

#[tokio::test]
async fn a_half_done_push_deletes_its_branch() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let fake = Arc::new(scripted().failing_commits());
    let github = guarded(&fake);
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let err = forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::GitHub(_)));
    assert!(fake.calls().contains(&GitHubCall::DeleteBranch {
        repo: home(),
        branch: "Ada/forja-department".into()
    }));
}

#[tokio::test]
async fn the_guard_refuses_pushes_to_main_in_any_spelling() {
    let fake = fake();
    let github = guarded(&fake);
    for branch in [
        "main",
        "Main",
        "master",
        "refs/heads/main",
        "heads/main",
        " main ",
    ] {
        let err = github
            .commit_file(&home(), branch, "a", "b", "c")
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitHubError::Status { status: 403, .. }),
            "{branch}"
        );
        assert!(github.create_branch(&home(), branch, "main").await.is_err());
        assert!(github.delete_branch(&home(), branch).await.is_err());
    }
    assert!(fake
        .calls()
        .iter()
        .all(|c| matches!(c, GitHubCall::Repository(_))));

    // A topic branch is fine.
    github
        .create_branch(&home(), "Ada/topic", "main")
        .await
        .unwrap();
}

#[tokio::test]
async fn the_guard_refuses_the_real_default_branch() {
    let fake = Arc::new(ScriptedGitHub::new("Gigajn").repository(RepoInfo {
        repo: home(),
        description: None,
        default_branch: "trunk".into(),
    }));
    let github = guarded(&fake);
    for branch in ["trunk", "refs/heads/trunk"] {
        let err = github
            .commit_file(&home(), branch, "a", "b", "c")
            .await
            .unwrap_err();
        assert!(
            matches!(err, GitHubError::Status { status: 403, .. }),
            "{branch}"
        );
    }
}

#[tokio::test]
async fn the_guard_refuses_merges_through_the_github_interface() {
    let fake = fake();
    let github = guarded(&fake);
    let dynamic: &dyn GitHub = &github;
    let err = dynamic.merge_pull_request(&home(), 7).await.unwrap_err();
    assert!(matches!(err, GitHubError::Status { status: 403, .. }));
    assert!(!fake
        .calls()
        .iter()
        .any(|c| matches!(c, GitHubCall::MergePullRequest { .. })));
    // Unguarded, the fake would have merged: the refusal is the guard's.
    fake.merge_pull_request(&home(), 7).await.unwrap();
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn a_pending_pull_request_waits_in_the_approval_queue() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = guarded(&fake());
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let made = forja
        .build(&sandbox, &github, &meter, "Ada", &ticket(), work())
        .await
        .unwrap();

    let approver = ScriptedApprover::new([]);
    let decision = forja.submit_for_review(&approver, &made).await;
    assert_eq!(decision, ApprovalDecision::Pending);
    let request = &approver.requests()[0];
    assert_eq!(request.job, "forja");
    assert_eq!(request.node, "pull-request/7");
    assert!(request
        .prompt
        .contains("https://github.com/Gigajn/Archodin_Inc/pull/7"));
    assert!(request.prompt.contains("Add Forja"));
    assert!(request.prompt.contains("#31"));
}
