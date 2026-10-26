use archodin::graph::{
    ApprovalDecision, Explorador, Forja, ForjaError, ForjaGitHub, ForjaWork, GitHub, GitHubCall,
    GitHubError, Issue, JobQueue, Nakama, RepoInfo, RepoRef, Role, Roster, SandboxError,
    SandboxOutput, ScriptedApprover, ScriptedGitHub, ScriptedSandbox, UsageMeter,
};

fn home() -> RepoRef {
    RepoRef::new("Gigajn", "Archodin_Inc")
}

fn fake() -> ScriptedGitHub {
    ScriptedGitHub::new("Gigajn")
        .repository(RepoInfo {
            repo: home(),
            description: Some("Archodin".into()),
            default_branch: "main".into(),
        })
        .issue(ticket())
        .issue(Issue::new(40, "Not ready", "", "u/40"))
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

async fn forja(meter: &UsageMeter) -> Forja {
    let roster = Roster::new();
    roster.add_role(Role::new("Contributor", "Build Archodin"));
    let nakama = Nakama::new(["Ada"], |_: &str| Role::new("X", "x"));
    let explorador = Explorador::new(["devtools"]);
    let charter = explorador
        .charter_from_link(&fake(), "Gigajn/Archodin_Inc")
        .await
        .unwrap();
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let department = explorador
        .approve(charter, &approver, &roster, &nakama, meter)
        .await
        .unwrap()
        .unwrap();
    Forja::new(department).unwrap()
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
async fn only_ready_tickets_become_jobs_once() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = ForjaGitHub::new(fake(), home());
    let queue = JobQueue::new();
    let ids = forja
        .pick_up(&github, &queue, &meter, |i| i.number == 31, |i| i.clone())
        .await
        .unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(queue.next().unwrap().state.number, 31);
    let again = forja
        .pick_up(&github, &queue, &meter, |i| i.number == 31, |i| i.clone())
        .await
        .unwrap();
    assert!(again.is_empty());
}

#[tokio::test]
async fn a_passing_build_opens_one_pull_request_that_links_the_ticket_and_says_ai() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = ForjaGitHub::new(fake(), home());
    let sandbox = ScriptedSandbox::new([ok(0)]);

    let made = forja
        .build(&sandbox, &github, "Ada", &ticket(), work())
        .await
        .unwrap();
    assert_eq!(made.branch, "Ada/forja-department");
    assert_eq!(made.pull_request.number, 7);
    assert_eq!(sandbox.commands(), ["cargo test"]);

    let calls = github.inner().calls();
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

#[tokio::test]
async fn a_failing_check_opens_nothing() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = ForjaGitHub::new(fake(), home());
    let sandbox = ScriptedSandbox::new([ok(101)]);
    let err = forja
        .build(&sandbox, &github, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ForjaError::CheckFailed { exit_code: 101, .. }
    ));
    assert!(github.inner().calls().is_empty());

    let broken = ScriptedSandbox::new([Err(SandboxError::Runtime("boom".into()))]);
    let err = forja
        .build(&broken, &github, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::Sandbox(_)));
    assert!(github.inner().calls().is_empty());
}

#[tokio::test]
async fn only_the_departments_agents_build() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = ForjaGitHub::new(fake(), home());
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let err = forja
        .build(&sandbox, &github, "Mallory", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(err, ForjaError::NotMember(_)));
    assert!(sandbox.commands().is_empty());
}

#[tokio::test]
async fn the_guard_refuses_pushes_to_main_and_merges() {
    let github = ForjaGitHub::new(fake(), home());
    for branch in ["main", "Main", "master"] {
        let err = github
            .commit_file(&home(), branch, "a", "b", "c")
            .await
            .unwrap_err();
        assert!(matches!(err, GitHubError::Status { status: 403, .. }));
        assert!(github.create_branch(&home(), branch, "main").await.is_err());
    }
    let err = github.merge_pull_request(&home(), 7).await.unwrap_err();
    assert!(matches!(err, GitHubError::Status { status: 403, .. }));
    // Nothing reached the wrapped GitHub.
    assert!(github.inner().calls().is_empty());

    // A topic branch is fine.
    github
        .create_branch(&home(), "Ada/topic", "main")
        .await
        .unwrap();
    assert_eq!(github.inner().calls().len(), 1);
}

#[tokio::test]
async fn a_protected_branch_stops_a_build() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let guarded = ForjaGitHub::new(fake(), home()).protect("Ada/forja-department");
    let err = forja
        .build(&sandbox, &guarded, "Ada", &ticket(), work())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ForjaError::GitHub(GitHubError::Status { status: 403, .. })
    ));
}

#[tokio::test]
async fn a_pending_pull_request_waits_in_the_approval_queue() {
    let meter = UsageMeter::new();
    let forja = forja(&meter).await;
    let github = ForjaGitHub::new(fake(), home());
    let sandbox = ScriptedSandbox::new([ok(0)]);
    let made = forja
        .build(&sandbox, &github, "Ada", &ticket(), work())
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
