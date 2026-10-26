use std::sync::Arc;

use archodin::graph::{
    ApprovalDecision, Budget, Contribution, Department, Explorador, ExploradorError, GitHub,
    GitHubCall, GitHubClient, GitHubError, Issue, JobOrigin, JobQueue, Nakama, Opportunity,
    RepoInfo, RepoRef, Role, Roster, ScriptedApprover, ScriptedGitHub, Usage, UsageMeter,
};
use mockito::{Matcher, Server};

fn upstream() -> RepoRef {
    RepoRef::new("acme", "widgets")
}

fn fake() -> ScriptedGitHub {
    ScriptedGitHub::new("kudo")
        .repository(RepoInfo {
            repo: upstream(),
            description: Some("A widget toolkit".into()),
            default_branch: "main".into(),
        })
        .issue(Issue::new(1, "Crash on empty input", "Steps...", "u/1"))
        .issue(Issue::new(2, "Typo in docs", "", "u/2"))
}

fn roster() -> Roster {
    let roster = Roster::new();
    roster.add_role(Role::new(
        "Contributor",
        "Fix issues in open-source projects",
    ));
    roster
}

fn nakama() -> Nakama {
    Nakama::new(["Ada", "Bo", "Cy"], |_: &str| Role::new("X", "x"))
}

fn explorador() -> Explorador {
    Explorador::new(["open-source", "devtools"])
}

#[test]
fn only_opportunities_in_the_configured_fields_become_charters() {
    let found = vec![
        Opportunity::new("open-source", "Help widgets", "Fix widgets bugs").role("Contributor", 2),
        Opportunity::new("crypto", "Trade coins", "Make money"),
        Opportunity::new("DevTools", "Linter", "Build a linter"),
    ];
    let charters = explorador().propose(found);
    let goals: Vec<_> = charters.iter().map(|c| c.goal()).collect();
    assert_eq!(goals, ["Fix widgets bugs", "Build a linter"]);
    assert_eq!(charters[0].roles(), [("Contributor".to_string(), 2)]);
}

#[tokio::test]
async fn a_pasted_github_link_produces_a_draft_charter_and_creates_nothing() {
    let github = fake();
    let charter = explorador()
        .charter_from_link(&github, "https://github.com/acme/widgets.git")
        .await
        .unwrap();
    assert_eq!(charter.source(), Some(&upstream()));
    assert!(charter.goal().contains("acme/widgets"));
    assert!(charter.goal().contains("A widget toolkit"));
    assert!(!charter.roles().is_empty());
    // Only the repository was read.
    assert_eq!(github.calls(), [GitHubCall::Repository(upstream())]);

    let bad = explorador()
        .charter_from_link(&github, "https://example.com/x")
        .await;
    assert!(matches!(
        bad,
        Err(ExploradorError::GitHub(GitHubError::InvalidLink(_)))
    ));
}

#[tokio::test]
async fn nothing_is_created_until_approved_and_approval_asks_nakama_for_agents() {
    let github = fake();
    let charter = explorador()
        .charter_from_link(&github, "github.com/acme/widgets")
        .await
        .unwrap();
    let roster = roster();
    let meter = UsageMeter::new();

    for decision in [ApprovalDecision::Rejected, ApprovalDecision::Pending] {
        let approver = ScriptedApprover::new([decision]);
        let made = explorador()
            .approve(charter.clone(), &approver, &roster, &nakama(), &meter)
            .await
            .unwrap();
        assert!(made.is_none());
        assert!(roster.agents().is_empty());
        assert_eq!(approver.requests()[0].node, "charter");
        assert!(approver.requests()[0].prompt.contains("widgets"));
    }

    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let department = explorador()
        .approve(charter, &approver, &roster, &nakama(), &meter)
        .await
        .unwrap()
        .expect("approved");
    assert_eq!(department.agents(), ["Ada"]);
    assert_eq!(roster.agents().len(), 1);
    assert_eq!(roster.agent("Ada").unwrap().role(), "Contributor");
}

async fn open_department(roster: &Roster, meter: &UsageMeter, budget: Budget) -> Department {
    let github = fake();
    let charter = explorador()
        .charter_from_link(&github, "acme/widgets")
        .await
        .unwrap()
        .with_budget(budget);
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    explorador()
        .approve(charter, &approver, roster, &nakama(), meter)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn issues_become_jobs_once_and_agora_jobs_run_first() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let department = open_department(&roster, &meter, Budget::tokens(1000)).await;
    let github = fake();
    let queue = JobQueue::new();
    queue.submit(JobOrigin::Issue, "older issue".to_string());

    let ids = department
        .sync_issues(&github, &queue, &meter, |i| format!("issue #{}", i.number))
        .await
        .unwrap();
    assert_eq!(ids.len(), 2);
    queue.submit(JobOrigin::Agora, "from Kudo".to_string());

    // A second sync adds nothing: the issues already are Jobs.
    let again = department
        .sync_issues(&github, &queue, &meter, |i| format!("issue #{}", i.number))
        .await
        .unwrap();
    assert!(again.is_empty());

    let order: Vec<_> = std::iter::from_fn(|| queue.next())
        .map(|j| j.state)
        .collect();
    assert_eq!(order, ["from Kudo", "older issue", "issue #1", "issue #2"]);
}

#[tokio::test]
async fn no_issue_jobs_are_created_once_the_department_budget_is_spent() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let department = open_department(&roster, &meter, Budget::tokens(100)).await;
    meter.record("job", &department.budget_key(), Usage::new(90, 60, 0.0));
    let queue: JobQueue<String> = JobQueue::new();
    let ids = department
        .sync_issues(&fake(), &queue, &meter, |i| i.title.clone())
        .await
        .unwrap();
    assert!(ids.is_empty() && queue.is_empty());
}

#[tokio::test]
async fn contributions_go_fork_then_named_branch_then_pull_request_with_ai_disclosure() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let department = open_department(&roster, &meter, Budget::tokens(1000)).await;
    let github = fake();

    let pr = department
        .contribute(
            &github,
            "Ada",
            Contribution::new("fix-empty-input", "Fix crash on empty input", "Closes #1").file(
                "src/lib.rs",
                "fn fixed() {}",
                "Handle empty input",
            ),
        )
        .await
        .unwrap();
    assert_eq!(pr.number, 7);

    let calls = github.calls();
    let fork = RepoRef::new("kudo", "widgets");
    assert_eq!(calls[0], GitHubCall::Repository(upstream()));
    assert_eq!(calls[1], GitHubCall::Fork(upstream()));
    assert_eq!(
        calls[2],
        GitHubCall::CreateBranch {
            repo: fork.clone(),
            branch: "Ada/fix-empty-input".into(),
            from: "main".into()
        }
    );
    assert!(
        matches!(&calls[3], GitHubCall::CommitFile { repo, branch, path, .. }
        if *repo == fork && branch == "Ada/fix-empty-input" && path == "src/lib.rs")
    );
    let GitHubCall::OpenPullRequest {
        upstream: target,
        draft,
    } = &calls[4]
    else {
        panic!("expected a pull request, got {:?}", calls[4]);
    };
    assert_eq!(*target, upstream());
    assert_eq!(draft.head, "kudo:Ada/fix-empty-input");
    assert_eq!(draft.base, "main");
    assert!(draft.body.contains("Closes #1"));
    assert!(draft.body.to_lowercase().contains("made with ai"));
    assert!(draft.body.contains("Ada"));
    // Nothing was written to the upstream repository.
    assert!(calls.iter().all(|c| !c.writes_to(&upstream())));
}

#[tokio::test]
async fn only_the_departments_own_agents_may_contribute() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let department = open_department(&roster, &meter, Budget::tokens(1000)).await;
    let github = fake();
    let err = department
        .contribute(&github, "Mallory", Contribution::new("x", "t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, ExploradorError::NotMember(_)));
    assert!(github.calls().is_empty());
}

#[tokio::test]
async fn the_fake_refuses_writes_to_a_repository_that_is_not_the_persons() {
    let github = fake();
    let err = github
        .create_branch(&upstream(), "Ada/x", "main")
        .await
        .unwrap_err();
    assert!(matches!(err, GitHubError::NotYourRepository(_)));
}

fn client(server: &Server) -> GitHubClient {
    GitHubClient::new(server.url(), "tok", "kudo")
}

#[tokio::test]
async fn real_client_reads_repository_and_open_issues_skipping_pull_requests() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/repos/acme/widgets")
        .match_header("authorization", "Bearer tok")
        .with_body(r#"{"description":"A widget toolkit","default_branch":"trunk"}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/repos/acme/widgets/issues")
        .match_query(Matcher::UrlEncoded("state".into(), "open".into()))
        .with_body(
            r#"[{"number":1,"title":"Bug","body":null,"html_url":"u/1"},
                {"number":2,"title":"A PR","body":"x","html_url":"u/2","pull_request":{}}]"#,
        )
        .create_async()
        .await;
    let github = client(&server);
    let info = github.repository(&upstream()).await.unwrap();
    assert_eq!(info.default_branch, "trunk");
    assert_eq!(info.description.as_deref(), Some("A widget toolkit"));
    let issues = github.open_issues(&upstream()).await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].number, 1);
}

#[tokio::test]
async fn real_client_forks_branches_commits_and_opens_the_pull_request() {
    let mut server = Server::new_async().await;
    let fork = server
        .mock("POST", "/repos/acme/widgets/forks")
        .with_status(202)
        .with_body(r#"{"name":"widgets","owner":{"login":"kudo"}}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/repos/kudo/widgets/git/ref/heads/main")
        .with_body(r#"{"object":{"sha":"abc123"}}"#)
        .create_async()
        .await;
    let branch = server
        .mock("POST", "/repos/kudo/widgets/git/refs")
        .match_body(Matcher::Json(
            serde_json::json!({"ref":"refs/heads/Ada/fix","sha":"abc123"}),
        ))
        .with_status(201)
        .with_body("{}")
        .create_async()
        .await;
    let commit = server
        .mock("PUT", "/repos/kudo/widgets/contents/src/lib.rs")
        .match_body(Matcher::Json(serde_json::json!({
            "message":"Handle empty input","content":"aGk=","branch":"Ada/fix"})))
        .with_status(201)
        .with_body("{}")
        .create_async()
        .await;
    let pull = server
        .mock("POST", "/repos/acme/widgets/pulls")
        .match_body(Matcher::PartialJson(serde_json::json!({
            "head":"kudo:Ada/fix","base":"main","title":"T"})))
        .with_status(201)
        .with_body(r#"{"number":9,"html_url":"https://github.com/acme/widgets/pull/9"}"#)
        .create_async()
        .await;

    let github: Arc<dyn GitHub> = Arc::new(client(&server));
    let mine = github.fork(&upstream()).await.unwrap();
    assert_eq!(mine, RepoRef::new("kudo", "widgets"));
    github
        .create_branch(&mine, "Ada/fix", "main")
        .await
        .unwrap();
    github
        .commit_file(&mine, "Ada/fix", "src/lib.rs", "hi", "Handle empty input")
        .await
        .unwrap();
    let pr = github
        .open_pull_request(
            &upstream(),
            archodin::graph::PullRequestDraft {
                head: "kudo:Ada/fix".into(),
                base: "main".into(),
                title: "T".into(),
                body: "B".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(pr.number, 9);
    for m in [fork, branch, commit, pull] {
        m.assert_async().await;
    }
}

#[tokio::test]
async fn real_client_never_writes_to_someone_elses_repository() {
    let server = Server::new_async().await; // no mocks: any request would fail
    let github = client(&server);
    let err = github
        .create_branch(&upstream(), "Ada/x", "main")
        .await
        .unwrap_err();
    assert!(matches!(err, GitHubError::NotYourRepository(_)));
    let err = github
        .commit_file(&upstream(), "Ada/x", "a", "b", "m")
        .await
        .unwrap_err();
    assert!(matches!(err, GitHubError::NotYourRepository(_)));
}

#[tokio::test]
async fn real_client_reports_http_errors() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/repos/acme/widgets")
        .with_status(404)
        .with_body("nope")
        .create_async()
        .await;
    let err = client(&server).repository(&upstream()).await.unwrap_err();
    assert!(matches!(err, GitHubError::Status { status: 404, .. }));
}
