//! Explorador, the Agent that grows the company by opening new Departments.
//!
//! [`Explorador`] proposes each new Department as a [`Charter`] (goal, where its
//! work comes from, Roles, Budget), either from [`Opportunity`]s inside the fields
//! a person configured or from a pasted GitHub repository link. Nothing exists
//! until a person approves the Charter ([`Explorador::approve`]); approval creates
//! the [`Department`] and asks Nakama for its Agents.
//!
//! An open-source Department turns its repository's issues into Jobs within its
//! Budget ([`Department::sync_issues`]; Agora Jobs still run first) and contributes
//! through [`Department::contribute`]: fork, `<agent-name>/<topic>` branch, pull
//! request with an AI disclosure. The [`GitHub`] trait has no way to push to
//! another project's branches, so nothing can reach an upstream main branch.
//! [`ScriptedGitHub`] is the test double; [`GitHubClient`] talks to the GitHub
//! REST API.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;

use crate::{
    branch_name, ApprovalDecision, ApprovalRequest, Approver, Budget, JobId, JobOrigin, JobQueue,
    Nakama, Roster, UsageMeter, Workload,
};

/// A GitHub repository: owner and name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoRef {
    /// The account or organization that owns the repository.
    pub owner: String,
    /// The repository name.
    pub name: String,
}

impl RepoRef {
    /// Builds a reference to `owner/name`.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }

    /// Reads `https://github.com/owner/name[.git][/...]`, `github.com/owner/name`
    /// or `owner/name`.
    pub fn parse(link: &str) -> Result<Self, GitHubError> {
        let bad = || GitHubError::InvalidLink(link.to_string());
        let mut rest = link.trim();
        for prefix in ["https://", "http://"] {
            rest = rest.strip_prefix(prefix).unwrap_or(rest);
        }
        if rest.contains('.') && rest.split('/').next().is_some_and(|h| h.contains('.')) {
            rest = rest
                .strip_prefix("www.github.com/")
                .or_else(|| rest.strip_prefix("github.com/"))
                .ok_or_else(bad)?;
        }
        let mut parts = rest.split(['/', '?', '#']);
        let owner = parts.next().filter(|s| !s.is_empty()).ok_or_else(bad)?;
        let name = parts.next().filter(|s| !s.is_empty()).ok_or_else(bad)?;
        let name = name.strip_suffix(".git").unwrap_or(name);
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if ok(owner) && ok(name) {
            Ok(Self::new(owner, name))
        } else {
            Err(bad())
        }
    }
}

impl std::fmt::Display for RepoRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// What GitHub says about a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// The repository.
    pub repo: RepoRef,
    /// Its description, if it has one.
    pub description: Option<String>,
    /// The branch pull requests target.
    pub default_branch: String,
}

/// An open issue of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The issue number.
    pub number: u64,
    /// The title.
    pub title: String,
    /// The body text (empty when there is none).
    pub body: String,
    /// Where to read it.
    pub url: String,
}

impl Issue {
    /// Builds an issue.
    pub fn new(
        number: u64,
        title: impl Into<String>,
        body: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            number,
            title: title.into(),
            body: body.into(),
            url: url.into(),
        }
    }
}

/// A pull request to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    /// `fork-owner:branch`.
    pub head: String,
    /// The branch of the upstream repository to merge into.
    pub base: String,
    /// The title.
    pub title: String,
    /// The description.
    pub body: String,
}

/// An opened pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// The pull request number.
    pub number: u64,
    /// Where to read it.
    pub url: String,
}

/// A pull request that is still open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPullRequest {
    /// The pull request number.
    pub number: u64,
    /// The head branch.
    pub head: String,
    /// The description.
    pub body: String,
}

/// Why a GitHub call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitHubError {
    /// The text is not a GitHub repository link.
    #[error("`{0}` is not a GitHub repository link")]
    InvalidLink(String),
    /// A write was aimed at a repository that does not belong to the person.
    #[error("refusing to write to `{0}`: it is not the person's repository")]
    NotYourRepository(RepoRef),
    /// The request did not complete.
    #[error("GitHub request failed: {0}")]
    Http(String),
    /// GitHub answered with an error status.
    #[error("GitHub answered {status}: {body}")]
    Status {
        /// The HTTP status.
        status: u16,
        /// The response body.
        body: String,
    },
}

/// The GitHub operations Departments need. There is deliberately no way to push
/// to a repository that is not the person's own: branches and files go to the
/// fork, and upstream only ever receives a pull request.
pub trait GitHub: Send + Sync {
    /// Reads a repository.
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>>;
    /// Lists the open issues (pull requests excluded).
    fn open_issues(&self, repo: &RepoRef) -> BoxFuture<'_, Result<Vec<Issue>, GitHubError>>;
    /// Forks `repo` into the person's account and returns the fork.
    fn fork(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoRef, GitHubError>>;
    /// Creates `branch` in the person's repository `repo`, starting at `from`.
    fn create_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
        from: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>>;
    /// Commits a new file to `branch` of the person's repository `repo`.
    fn commit_file(
        &self,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        content: &str,
        message: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>>;
    /// Opens a pull request against `upstream`.
    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>>;
    /// Deletes `branch` of the person's repository `repo`.
    fn delete_branch(&self, repo: &RepoRef, branch: &str)
        -> BoxFuture<'_, Result<(), GitHubError>>;
    /// Lists the open pull requests of `repo`.
    fn open_pull_requests(
        &self,
        repo: &RepoRef,
    ) -> BoxFuture<'_, Result<Vec<OpenPullRequest>, GitHubError>>;
    /// Merges pull request `number` of `repo`. Only a person merges: Departments
    /// are never given a GitHub that does (Forja's guard always refuses), so the
    /// default is to not support it.
    fn merge_pull_request(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let err = GitHubError::Http(format!(
            "merging pull request #{number} of {repo} is not supported"
        ));
        Box::pin(async move { Err(err) })
    }
}

/// One call received by a [`ScriptedGitHub`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubCall {
    /// `repository`.
    Repository(RepoRef),
    /// `open_issues`.
    OpenIssues(RepoRef),
    /// `fork`.
    Fork(RepoRef),
    /// `create_branch`.
    CreateBranch {
        /// The repository.
        repo: RepoRef,
        /// The new branch.
        branch: String,
        /// The branch it starts from.
        from: String,
    },
    /// `commit_file`.
    CommitFile {
        /// The repository.
        repo: RepoRef,
        /// The branch.
        branch: String,
        /// The file path.
        path: String,
        /// The file content.
        content: String,
        /// The commit message.
        message: String,
    },
    /// `open_pull_request`.
    OpenPullRequest {
        /// The repository the pull request targets.
        upstream: RepoRef,
        /// The pull request.
        draft: PullRequestDraft,
    },
    /// `delete_branch`.
    DeleteBranch {
        /// The repository.
        repo: RepoRef,
        /// The deleted branch.
        branch: String,
    },
    /// `open_pull_requests`.
    OpenPullRequests(RepoRef),
    /// `merge_pull_request`.
    MergePullRequest {
        /// The repository.
        repo: RepoRef,
        /// The pull request number.
        number: u64,
    },
}

impl GitHubCall {
    /// Whether this call put commits or branches into `repo`.
    pub fn writes_to(&self, repo: &RepoRef) -> bool {
        matches!(self, Self::CreateBranch { repo: r, .. } | Self::CommitFile { repo: r, .. } if r == repo)
    }
}

#[derive(Default)]
struct ScriptedGitHubState {
    repos: Vec<RepoInfo>,
    issues: Vec<Issue>,
    pulls: Vec<OpenPullRequest>,
    fail_commits: bool,
    calls: Vec<GitHubCall>,
}

/// A test double for [`GitHub`]: serves the repositories and issues it is given,
/// records every call, and refuses writes outside the person's account like the
/// real client does. Pull requests are numbered 7.
pub struct ScriptedGitHub {
    login: String,
    state: Mutex<ScriptedGitHubState>,
}

impl ScriptedGitHub {
    /// A fake GitHub for the person whose account is `login`.
    pub fn new(login: impl Into<String>) -> Self {
        Self {
            login: login.into(),
            state: Mutex::new(ScriptedGitHubState::default()),
        }
    }

    /// Makes a repository readable.
    pub fn repository(self, info: RepoInfo) -> Self {
        self.state().repos.push(info);
        self
    }

    /// Adds an open issue (served for every repository).
    pub fn issue(self, issue: Issue) -> Self {
        self.state().issues.push(issue);
        self
    }

    /// Adds an already open pull request (served for every repository).
    pub fn open_pull_request_entry(self, pull: OpenPullRequest) -> Self {
        self.state().pulls.push(pull);
        self
    }

    /// Makes every `commit_file` fail, to test clean-up after a half-done push.
    pub fn failing_commits(self) -> Self {
        self.state().fail_commits = true;
        self
    }

    /// Every call received so far, in order.
    pub fn calls(&self) -> Vec<GitHubCall> {
        self.state().calls.clone()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ScriptedGitHubState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn own(&self, repo: &RepoRef) -> Result<(), GitHubError> {
        if repo.owner == self.login {
            Ok(())
        } else {
            Err(GitHubError::NotYourRepository(repo.clone()))
        }
    }
}

impl GitHub for ScriptedGitHub {
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>> {
        let mut s = self.state();
        s.calls.push(GitHubCall::Repository(repo.clone()));
        let found = s.repos.iter().find(|i| i.repo == *repo).cloned();
        Box::pin(async move {
            found.ok_or_else(|| GitHubError::Status {
                status: 404,
                body: "not found".into(),
            })
        })
    }

    fn open_issues(&self, repo: &RepoRef) -> BoxFuture<'_, Result<Vec<Issue>, GitHubError>> {
        let mut s = self.state();
        s.calls.push(GitHubCall::OpenIssues(repo.clone()));
        let issues = s.issues.clone();
        Box::pin(async move { Ok(issues) })
    }

    fn fork(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoRef, GitHubError>> {
        self.state().calls.push(GitHubCall::Fork(repo.clone()));
        let fork = RepoRef::new(self.login.clone(), repo.name.clone());
        Box::pin(async move { Ok(fork) })
    }

    fn create_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
        from: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let result = self.own(repo);
        if result.is_ok() {
            self.state().calls.push(GitHubCall::CreateBranch {
                repo: repo.clone(),
                branch: branch.into(),
                from: from.into(),
            });
        }
        Box::pin(async move { result })
    }

    fn commit_file(
        &self,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        content: &str,
        message: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let mut result = self.own(repo);
        if result.is_ok() && self.state().fail_commits {
            result = Err(GitHubError::Http("commit failed".into()));
        }
        if result.is_ok() {
            self.state().calls.push(GitHubCall::CommitFile {
                repo: repo.clone(),
                branch: branch.into(),
                path: path.into(),
                content: content.into(),
                message: message.into(),
            });
        }
        Box::pin(async move { result })
    }

    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>> {
        self.state().calls.push(GitHubCall::OpenPullRequest {
            upstream: upstream.clone(),
            draft,
        });
        let url = format!("https://github.com/{upstream}/pull/7");
        Box::pin(async move { Ok(PullRequest { number: 7, url }) })
    }

    fn delete_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let result = self.own(repo);
        if result.is_ok() {
            self.state().calls.push(GitHubCall::DeleteBranch {
                repo: repo.clone(),
                branch: branch.into(),
            });
        }
        Box::pin(async move { result })
    }

    fn open_pull_requests(
        &self,
        repo: &RepoRef,
    ) -> BoxFuture<'_, Result<Vec<OpenPullRequest>, GitHubError>> {
        let mut s = self.state();
        s.calls.push(GitHubCall::OpenPullRequests(repo.clone()));
        let pulls = s.pulls.clone();
        Box::pin(async move { Ok(pulls) })
    }

    fn merge_pull_request(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        self.state().calls.push(GitHubCall::MergePullRequest {
            repo: repo.clone(),
            number,
        });
        Box::pin(async { Ok(()) })
    }
}

/// The real [`GitHub`], over the GitHub REST API with the person's token.
pub struct GitHubClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    login: String,
}

impl GitHubClient {
    /// `base_url` is `https://api.github.com` (or a test server); `login` is the
    /// person's account, the only one this client writes to.
    pub fn new(
        base_url: impl Into<String>,
        token: impl Into<String>,
        login: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
            login: login.into(),
        }
    }

    fn own(&self, repo: &RepoRef) -> Result<(), GitHubError> {
        if repo.owner == self.login {
            Ok(())
        } else {
            Err(GitHubError::NotYourRepository(repo.clone()))
        }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, GitHubError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .header("user-agent", "archodin");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| GitHubError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(GitHubError::Status {
                status: status.as_u16(),
                body: text,
            });
        }
        serde_json::from_str(&text).map_err(|e| GitHubError::Http(format!("bad JSON: {e}")))
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, GitHubError> {
    serde_json::from_value(value).map_err(|e| GitHubError::Http(format!("unexpected reply: {e}")))
}

impl GitHub for GitHubClient {
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>> {
        let repo = repo.clone();
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Reply {
                description: Option<String>,
                default_branch: String,
            }
            let reply: Reply = decode(
                self.send(reqwest::Method::GET, &format!("/repos/{repo}"), None)
                    .await?,
            )?;
            Ok(RepoInfo {
                repo,
                description: reply.description,
                default_branch: reply.default_branch,
            })
        })
    }

    fn open_issues(&self, repo: &RepoRef) -> BoxFuture<'_, Result<Vec<Issue>, GitHubError>> {
        let repo = repo.clone();
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Reply {
                number: u64,
                title: String,
                body: Option<String>,
                html_url: String,
                pull_request: Option<serde_json::Value>,
            }
            let replies: Vec<Reply> = decode(
                self.send(
                    reqwest::Method::GET,
                    &format!("/repos/{repo}/issues?state=open&per_page=100"),
                    None,
                )
                .await?,
            )?;
            Ok(replies
                .into_iter()
                .filter(|r| r.pull_request.is_none())
                .map(|r| Issue::new(r.number, r.title, r.body.unwrap_or_default(), r.html_url))
                .collect())
        })
    }

    fn fork(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoRef, GitHubError>> {
        let repo = repo.clone();
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Owner {
                login: String,
            }
            #[derive(Deserialize)]
            struct Reply {
                name: String,
                owner: Owner,
            }
            let reply: Reply = decode(
                self.send(
                    reqwest::Method::POST,
                    &format!("/repos/{repo}/forks"),
                    Some(json!({})),
                )
                .await?,
            )?;
            Ok(RepoRef::new(reply.owner.login, reply.name))
        })
    }

    fn create_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
        from: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let (repo, branch, from) = (repo.clone(), branch.to_string(), from.to_string());
        Box::pin(async move {
            self.own(&repo)?;
            let reply = self
                .send(
                    reqwest::Method::GET,
                    &format!("/repos/{repo}/git/ref/heads/{from}"),
                    None,
                )
                .await?;
            let sha = reply["object"]["sha"]
                .as_str()
                .ok_or_else(|| GitHubError::Http("reply has no object.sha".into()))?;
            self.send(
                reqwest::Method::POST,
                &format!("/repos/{repo}/git/refs"),
                Some(json!({"ref": format!("refs/heads/{branch}"), "sha": sha})),
            )
            .await?;
            Ok(())
        })
    }

    fn commit_file(
        &self,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        content: &str,
        message: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let (repo, branch, path) = (repo.clone(), branch.to_string(), path.to_string());
        let message = message.to_string();
        let content = base64::engine::general_purpose::STANDARD.encode(content);
        Box::pin(async move {
            self.own(&repo)?;
            self.send(
                reqwest::Method::PUT,
                &format!("/repos/{repo}/contents/{path}"),
                Some(json!({"message": message, "content": content, "branch": branch})),
            )
            .await?;
            Ok(())
        })
    }

    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>> {
        let upstream = upstream.clone();
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Reply {
                number: u64,
                html_url: String,
            }
            let reply: Reply = decode(
                self.send(
                    reqwest::Method::POST,
                    &format!("/repos/{upstream}/pulls"),
                    Some(json!({
                        "title": draft.title,
                        "head": draft.head,
                        "base": draft.base,
                        "body": draft.body,
                    })),
                )
                .await?,
            )?;
            Ok(PullRequest {
                number: reply.number,
                url: reply.html_url,
            })
        })
    }

    fn delete_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let (repo, branch) = (repo.clone(), branch.to_string());
        Box::pin(async move {
            self.own(&repo)?;
            self.send(
                reqwest::Method::DELETE,
                &format!("/repos/{repo}/git/refs/heads/{branch}"),
                None,
            )
            .await?;
            Ok(())
        })
    }

    fn open_pull_requests(
        &self,
        repo: &RepoRef,
    ) -> BoxFuture<'_, Result<Vec<OpenPullRequest>, GitHubError>> {
        let repo = repo.clone();
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Head {
                r#ref: String,
            }
            #[derive(Deserialize)]
            struct Reply {
                number: u64,
                body: Option<String>,
                head: Head,
            }
            let replies: Vec<Reply> = decode(
                self.send(
                    reqwest::Method::GET,
                    &format!("/repos/{repo}/pulls?state=open&per_page=100"),
                    None,
                )
                .await?,
            )?;
            Ok(replies
                .into_iter()
                .map(|r| OpenPullRequest {
                    number: r.number,
                    head: r.head.r#ref,
                    body: r.body.unwrap_or_default(),
                })
                .collect())
        })
    }
}

/// Why an Explorador or Department operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExploradorError {
    /// GitHub failed.
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    /// The Department has no linked repository.
    #[error("the Department has no linked repository")]
    NoRepository,
    /// The Agent does not belong to the Department.
    #[error("`{0}` is not an Agent of this Department")]
    NotMember(String),
}

/// A possible new Department Explorador found.
#[derive(Debug, Clone, PartialEq)]
pub struct Opportunity {
    field: String,
    name: String,
    goal: String,
    roles: Vec<(String, usize)>,
}

impl Opportunity {
    /// An opportunity in `field`, to be called `name`, with the given `goal`.
    pub fn new(field: impl Into<String>, name: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            name: name.into(),
            goal: goal.into(),
            roles: Vec::new(),
        }
    }

    /// Needs `agents` Agents of the Role called `role`.
    pub fn role(mut self, role: impl Into<String>, agents: usize) -> Self {
        self.roles.push((role.into(), agents));
        self
    }
}

/// A proposal for a new Department. Nothing exists until a person approves it.
#[derive(Debug, Clone, PartialEq)]
pub struct Charter {
    name: String,
    goal: String,
    source: Option<RepoRef>,
    roles: Vec<(String, usize)>,
    budget: Budget,
}

impl Charter {
    /// The proposed Department's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the Department is for.
    pub fn goal(&self) -> &str {
        &self.goal
    }

    /// The GitHub repository its work comes from, if any.
    pub fn source(&self) -> Option<&RepoRef> {
        self.source.as_ref()
    }

    /// Its Roles and how many Agents each needs.
    pub fn roles(&self) -> &[(String, usize)] {
        &self.roles
    }

    /// Its Budget.
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// Replaces the Budget.
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }

    /// What the person is asked to approve.
    pub fn describe(&self) -> String {
        let roles: Vec<_> = self
            .roles
            .iter()
            .map(|(r, n)| format!("{n} x {r}"))
            .collect();
        let source = self
            .source
            .as_ref()
            .map_or("no linked repository".to_string(), |r| {
                format!("work from github.com/{r}")
            });
        format!(
            "Open the Department {}: {} ({source}; Roles: {}; Budget: {:?})",
            self.name,
            self.goal,
            roles.join(", "),
            self.budget
        )
    }
}

/// The Agent that proposes new Departments.
pub struct Explorador {
    fields: Vec<String>,
    default_budget: Budget,
}

impl Explorador {
    /// Explorador looks for opportunities only in `fields`.
    pub fn new<F: Into<String>>(fields: impl IntoIterator<Item = F>) -> Self {
        Self {
            fields: fields.into_iter().map(Into::into).collect(),
            default_budget: Budget::tokens(100_000),
        }
    }

    /// Sets the Budget proposed for Charters drafted from a link.
    pub fn default_budget(mut self, budget: Budget) -> Self {
        self.default_budget = budget;
        self
    }

    /// Turns the opportunities inside the configured fields into Charters and
    /// drops the rest.
    pub fn propose(&self, found: impl IntoIterator<Item = Opportunity>) -> Vec<Charter> {
        found
            .into_iter()
            .filter(|o| self.fields.iter().any(|f| f.eq_ignore_ascii_case(&o.field)))
            .map(|o| Charter {
                name: o.name,
                goal: o.goal,
                source: None,
                roles: o.roles,
                budget: self.default_budget,
            })
            .collect()
    }

    /// Drafts a Charter from a pasted GitHub repository link. It only reads the
    /// repository; nothing is created.
    pub async fn charter_from_link(
        &self,
        github: &dyn GitHub,
        link: &str,
    ) -> Result<Charter, ExploradorError> {
        let repo = RepoRef::parse(link)?;
        let info = github.repository(&repo).await?;
        let goal = match info.description.as_deref().filter(|d| !d.is_empty()) {
            Some(d) => format!("Contribute to {repo} ({d})"),
            None => format!("Contribute to {repo}"),
        };
        Ok(Charter {
            name: repo.name.clone(),
            goal,
            source: Some(repo),
            roles: vec![("Contributor".into(), 1)],
            budget: self.default_budget,
        })
    }

    /// Asks `approver` about the Charter. Only when it is approved: the
    /// Department is created, its Budget registered on `meter`, and `nakama`
    /// hires its Agents from `roster`. Rejected or pending Charters create nothing.
    pub async fn approve(
        &self,
        charter: Charter,
        approver: &dyn Approver,
        roster: &Roster,
        nakama: &Nakama,
        meter: &UsageMeter,
    ) -> Result<Option<Department>, ExploradorError> {
        let request = ApprovalRequest {
            job: "explorador".into(),
            node: "charter".into(),
            prompt: charter.describe(),
        };
        if approver.decide(&request).await != ApprovalDecision::Approved {
            return Ok(None);
        }
        let department = Department {
            inner: Arc::new(DepartmentInner {
                charter,
                agents: Mutex::new(Vec::new()),
                seen_issues: Mutex::new(HashSet::new()),
            }),
        };
        meter.set_agent_budget(&department.budget_key(), department.inner.charter.budget);
        // The person approved the Charter, so the hires it names need no second ask.
        let approved = ScriptedAlways;
        for (role, count) in &department.inner.charter.roles {
            for _ in 0..*count {
                let workload = Workload::new().backlog(role.clone(), usize::MAX);
                let proposals = nakama.propose(roster, &workload);
                for applied in nakama.resolve(roster, proposals, &approved).await {
                    if let crate::Proposal::Hire { agent, .. } = applied {
                        lock(&department.inner.agents).push(agent);
                    }
                }
            }
        }
        Ok(Some(department))
    }
}

struct ScriptedAlways;

impl Approver for ScriptedAlways {
    fn decide(&self, _: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision> {
        Box::pin(async { ApprovalDecision::Approved })
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Work an Agent wants to contribute to the Department's repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contribution {
    topic: String,
    title: String,
    body: String,
    files: Vec<(String, String, String)>,
}

impl Contribution {
    /// A contribution on `topic` (it names the branch), with the pull request's
    /// title and body.
    pub fn new(
        topic: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            topic: topic.into(),
            title: title.into(),
            body: body.into(),
            files: Vec::new(),
        }
    }

    /// Adds a new file with its commit message.
    pub fn file(
        mut self,
        path: impl Into<String>,
        content: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.files
            .push((path.into(), content.into(), message.into()));
        self
    }
}

struct DepartmentInner {
    charter: Charter,
    agents: Mutex<Vec<String>>,
    seen_issues: Mutex<HashSet<u64>>,
}

/// A Department opened from an approved [`Charter`]. Clones share state.
#[derive(Clone)]
pub struct Department {
    inner: Arc<DepartmentInner>,
}

impl Department {
    /// The Department's name.
    pub fn name(&self) -> &str {
        &self.inner.charter.name
    }

    /// The Charter it was opened from.
    pub fn charter(&self) -> &Charter {
        &self.inner.charter
    }

    /// The Agents Nakama hired for it.
    pub fn agents(&self) -> Vec<String> {
        lock(&self.inner.agents).clone()
    }

    /// The name under which its spending is recorded on a [`UsageMeter`] (as an
    /// Agent key), where its Budget is registered.
    pub fn budget_key(&self) -> String {
        format!("department:{}", self.inner.charter.name)
    }

    /// Turns the repository's open issues into queued Jobs, once each, as long
    /// as the Department's Budget is not spent. `make` builds a Job's State from
    /// an issue. Jobs from the Agora still run first.
    pub async fn sync_issues<S>(
        &self,
        github: &dyn GitHub,
        queue: &JobQueue<S>,
        meter: &UsageMeter,
        make: impl Fn(&Issue) -> S,
    ) -> Result<Vec<JobId>, ExploradorError> {
        let repo = self
            .inner
            .charter
            .source
            .as_ref()
            .ok_or(ExploradorError::NoRepository)?;
        if meter.exceeded("", &self.budget_key()).is_some() {
            return Ok(Vec::new());
        }
        let issues = github.open_issues(repo).await?;
        let mut seen = lock(&self.inner.seen_issues);
        Ok(issues
            .iter()
            .filter(|i| seen.insert(i.number))
            .map(|i| queue.submit(JobOrigin::Issue, make(i)))
            .collect())
    }

    /// Contributes through a fork: the Agent's `<agent-name>/<topic>` branch in
    /// the person's fork, then a pull request that says it was made with AI.
    pub async fn contribute(
        &self,
        github: &dyn GitHub,
        agent: &str,
        contribution: Contribution,
    ) -> Result<PullRequest, ExploradorError> {
        let upstream = self
            .inner
            .charter
            .source
            .as_ref()
            .ok_or(ExploradorError::NoRepository)?;
        if !lock(&self.inner.agents).iter().any(|a| a == agent) {
            return Err(ExploradorError::NotMember(agent.into()));
        }
        let info = github.repository(upstream).await?;
        let fork = github.fork(upstream).await?;
        if fork == *upstream {
            return Err(GitHubError::NotYourRepository(fork).into());
        }
        let branch = branch_name(agent, &contribution.topic);
        github
            .create_branch(&fork, &branch, &info.default_branch)
            .await?;
        for (path, content, message) in &contribution.files {
            github
                .commit_file(&fork, &branch, path, content, message)
                .await?;
        }
        let body = format!(
            "{}\n\n---\nDisclosure: this contribution was made with AI by the Archodin Agent `{agent}`.",
            contribution.body
        );
        let draft = PullRequestDraft {
            head: format!("{}:{branch}", fork.owner),
            base: info.default_branch,
            title: contribution.title,
            body,
        };
        Ok(github.open_pull_request(upstream, draft).await?)
    }
}

impl Department {
    /// Records an Agent hired for the Department after it opened (used by upgrades).
    pub(crate) fn add_agent(&self, name: String) {
        lock(&self.inner.agents).push(name);
    }
}
