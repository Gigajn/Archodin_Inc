//! Forja: the Department that develops Archodin (ADR 0006).
//!
//! Forja takes tickets from the Archodin repository, builds in a [`Sandbox`], and
//! opens one pull request per ticket from an `<agent-name>/<topic>` branch. It
//! can never push to `main` or merge its own pull requests: every GitHub call it
//! makes goes through a [`ForjaGitHub`], which refuses them. A pending pull
//! request is put in front of a person as an [`ApprovalRequest`], so it waits in
//! the Agora's approval queue until a person merges it.
//!
//! The approval is not connected to the pull request's state on GitHub: a person
//! merges or closes the pull request on GitHub and answers the approval in the
//! Agora as two separate acts, and nothing yet closes the approval when the
//! pull request is merged or closed.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use crate::{
    branch_name, ApprovalDecision, ApprovalRequest, Approver, Department, GitHub, GitHubError,
    Issue, JobId, JobOrigin, JobQueue, OpenPullRequest, PullRequest, PullRequestDraft, RepoInfo,
    RepoRef, Sandbox, SandboxError, UsageMeter,
};

/// Branches Forja may never write to, whatever the repository.
const PROTECTED_BRANCHES: [&str; 2] = ["main", "master"];

fn forbidden(what: String) -> GitHubError {
    GitHubError::Status {
        status: 403,
        body: format!("Forja may not {what}"),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A branch name as git sees it, whatever the spelling (`refs/heads/main`,
/// `heads/main`, stray spaces, upper case).
fn normalize(branch: &str) -> String {
    let b = branch.trim();
    let b = b.strip_prefix("refs/").unwrap_or(b);
    let b = b.strip_prefix("heads/").unwrap_or(b);
    b.trim_matches('/').to_lowercase()
}

/// A [`GitHub`] that refuses what Forja must never do: write to `main`, to the
/// repository's real default branch or to any other protected branch (in any
/// spelling), and merge pull requests. Everything else is passed to the
/// wrapped GitHub, which Forja cannot reach any other way.
pub struct ForjaGitHub<G> {
    inner: G,
    home: RepoRef,
    protected: Mutex<Vec<String>>,
    default_branch: Mutex<Option<String>>,
}

impl<G: GitHub> ForjaGitHub<G> {
    /// Guards `inner` for Forja working on `home` (the Archodin repository).
    pub fn new(inner: G, home: RepoRef) -> Self {
        Self {
            inner,
            home,
            protected: Mutex::new(PROTECTED_BRANCHES.iter().map(|b| b.to_string()).collect()),
            default_branch: Mutex::new(None),
        }
    }

    /// Also refuses writes to `branch` (in any spelling).
    pub fn protect(self, branch: impl AsRef<str>) -> Self {
        lock(&self.protected).push(normalize(branch.as_ref()));
        self
    }

    /// The repository Forja works on.
    pub fn home(&self) -> &RepoRef {
        &self.home
    }

    /// Whether a write to `branch` of `repo` is allowed, as far as the guard
    /// knows (it learns the default branch of the home repository when it reads it).
    pub fn allows_branch(&self, repo: &RepoRef, branch: &str) -> Result<(), GitHubError> {
        let name = normalize(branch);
        let default = lock(&self.default_branch).clone();
        let is_default =
            *repo == self.home && default.as_deref().map(normalize) == Some(name.clone());
        if name.is_empty() || is_default || lock(&self.protected).contains(&name) {
            return Err(forbidden(format!("write to `{branch}` of {repo}")));
        }
        Ok(())
    }

    /// Checks a write, reading the home repository's default branch first when
    /// it is not known yet.
    async fn guard(&self, repo: &RepoRef, branch: &str) -> Result<(), GitHubError> {
        if *repo == self.home && lock(&self.default_branch).is_none() {
            self.repository(&self.home).await?;
        }
        self.allows_branch(repo, branch)
    }
}

impl<G: GitHub> GitHub for ForjaGitHub<G> {
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>> {
        let repo = repo.clone();
        Box::pin(async move {
            let info = self.inner.repository(&repo).await?;
            if repo == self.home {
                *lock(&self.default_branch) = Some(info.default_branch.clone());
            }
            Ok(info)
        })
    }

    fn open_issues(&self, repo: &RepoRef) -> BoxFuture<'_, Result<Vec<Issue>, GitHubError>> {
        self.inner.open_issues(repo)
    }

    fn fork(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoRef, GitHubError>> {
        self.inner.fork(repo)
    }

    fn create_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
        from: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let (repo, branch, from) = (repo.clone(), branch.to_string(), from.to_string());
        Box::pin(async move {
            self.guard(&repo, &branch).await?;
            self.inner.create_branch(&repo, &branch, &from).await
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
        let (content, message) = (content.to_string(), message.to_string());
        Box::pin(async move {
            self.guard(&repo, &branch).await?;
            self.inner
                .commit_file(&repo, &branch, &path, &content, &message)
                .await
        })
    }

    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>> {
        self.inner.open_pull_request(upstream, draft)
    }

    fn delete_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let (repo, branch) = (repo.clone(), branch.to_string());
        Box::pin(async move {
            self.guard(&repo, &branch).await?;
            self.inner.delete_branch(&repo, &branch).await
        })
    }

    fn open_pull_requests(
        &self,
        repo: &RepoRef,
    ) -> BoxFuture<'_, Result<Vec<OpenPullRequest>, GitHubError>> {
        self.inner.open_pull_requests(repo)
    }

    /// Always refused: only a person merges a pull request.
    fn merge_pull_request(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        let err = forbidden(format!("merge pull request #{number} of {repo}"));
        Box::pin(async move { Err(err) })
    }
}

/// A shared GitHub is a GitHub, so tests and callers can keep a handle on it.
impl<T: GitHub + ?Sized> GitHub for Arc<T> {
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>> {
        (**self).repository(repo)
    }

    fn open_issues(&self, repo: &RepoRef) -> BoxFuture<'_, Result<Vec<Issue>, GitHubError>> {
        (**self).open_issues(repo)
    }

    fn fork(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoRef, GitHubError>> {
        (**self).fork(repo)
    }

    fn create_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
        from: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        (**self).create_branch(repo, branch, from)
    }

    fn commit_file(
        &self,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        content: &str,
        message: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        (**self).commit_file(repo, branch, path, content, message)
    }

    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>> {
        (**self).open_pull_request(upstream, draft)
    }

    fn delete_branch(
        &self,
        repo: &RepoRef,
        branch: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        (**self).delete_branch(repo, branch)
    }

    fn open_pull_requests(
        &self,
        repo: &RepoRef,
    ) -> BoxFuture<'_, Result<Vec<OpenPullRequest>, GitHubError>> {
        (**self).open_pull_requests(repo)
    }

    fn merge_pull_request(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        (**self).merge_pull_request(repo, number)
    }
}

/// Why a Forja operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForjaError {
    /// The Department has no linked repository.
    #[error("the Department has no linked repository")]
    NoRepository,
    /// The Department's repository is not the one Forja develops.
    #[error("the Department points at {found}, not at {expected}")]
    WrongRepository {
        /// The Archodin repository.
        expected: RepoRef,
        /// The Department's repository.
        found: RepoRef,
    },
    /// The Agent does not belong to Forja.
    #[error("`{0}` is not an Agent of Forja")]
    NotMember(String),
    /// The Agent name or topic cannot make a branch name.
    #[error("`{0}` cannot make a branch name")]
    InvalidName(String),
    /// The Department's Budget is spent.
    #[error("the Department's Budget is spent")]
    BudgetExceeded,
    /// A check failed in the Sandbox, so no pull request was opened.
    #[error("check `{command}` failed with exit code {exit_code}: {output}")]
    CheckFailed {
        /// The command that failed.
        command: String,
        /// Its exit code.
        exit_code: i32,
        /// What it printed on standard output and standard error.
        output: String,
    },
    /// The Sandbox could not stage the files or run a check.
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    /// GitHub failed or refused.
    #[error(transparent)]
    GitHub(#[from] GitHubError),
}

/// What an Agent built for one ticket: files to commit and checks to pass first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForjaWork {
    topic: String,
    title: String,
    summary: String,
    files: Vec<(String, String, String)>,
    checks: Vec<String>,
}

impl ForjaWork {
    /// Work on `topic` (it names the branch), with the pull request's title and summary.
    pub fn new(
        topic: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            topic: topic.into(),
            title: title.into(),
            summary: summary.into(),
            files: Vec::new(),
            checks: Vec::new(),
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

    /// Adds a command that must exit 0 in the Sandbox, where the files are
    /// staged, before the pull request opens.
    pub fn check(mut self, command: impl Into<String>) -> Self {
        self.checks.push(command.into());
        self
    }
}

/// A pull request Forja opened for a ticket, waiting for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForjaPullRequest {
    /// The Agent that built it.
    pub agent: String,
    /// The ticket it answers.
    pub ticket: Issue,
    /// Its `<agent-name>/<topic>` branch.
    pub branch: String,
    /// The pull request's title.
    pub title: String,
    /// The pull request on GitHub.
    pub pull_request: PullRequest,
}

impl ForjaPullRequest {
    /// The question a person is asked: review and merge this pull request.
    pub fn approval_request(&self) -> ApprovalRequest {
        ApprovalRequest {
            job: "forja".into(),
            node: format!("pull-request/{}", self.pull_request.number),
            prompt: format!(
                "Review and merge Forja's pull request \"{}\" ({}), made with AI by `{}` for ticket #{} ({}). Nothing reaches main until you merge it.",
                self.title,
                self.pull_request.url,
                self.agent,
                self.ticket.number,
                self.ticket.url
            ),
        }
    }
}

/// Whether `body` says it closes ticket `number` (`Closes #31`, not `Closes #310`).
fn closes(body: &str, number: u64) -> bool {
    let key = format!("Closes #{number}");
    body.match_indices(&key).any(|(at, _)| {
        !body[at + key.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

fn valid_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

#[derive(Default)]
struct Tickets {
    claimed: HashSet<u64>,
    done: HashSet<u64>,
}

/// The Department that develops Archodin. Clones share state.
#[derive(Clone)]
pub struct Forja {
    department: Department,
    home: RepoRef,
    tickets: Arc<Mutex<Tickets>>,
}

impl Forja {
    /// Forja from an approved Department whose Charter points at `home`, the
    /// Archodin repository.
    pub fn new(department: Department, home: RepoRef) -> Result<Self, ForjaError> {
        match department.charter().source() {
            None => Err(ForjaError::NoRepository),
            Some(found) if *found != home => Err(ForjaError::WrongRepository {
                expected: home,
                found: found.clone(),
            }),
            Some(_) => Ok(Self {
                department,
                home,
                tickets: Arc::new(Mutex::new(Tickets::default())),
            }),
        }
    }

    /// The underlying Department.
    pub fn department(&self) -> &Department {
        &self.department
    }

    /// Queues a Job for every open ticket that `ready` accepts (the
    /// `ready-for-agent` ones), while the Department's Budget lasts. `make`
    /// builds a Job's State from a ticket. A ticket is skipped while its Job is
    /// queued or running, once its build succeeded, and when an open pull
    /// request already closes it; a failed build lets it be queued again.
    pub async fn pick_up<G: GitHub, S>(
        &self,
        github: &ForjaGitHub<G>,
        queue: &JobQueue<S>,
        meter: &UsageMeter,
        ready: impl Fn(&Issue) -> bool,
        make: impl Fn(&Issue) -> S,
    ) -> Result<Vec<JobId>, ForjaError> {
        if meter.exceeded("", &self.department.budget_key()).is_some() {
            return Ok(Vec::new());
        }
        let issues = github.open_issues(&self.home).await?;
        let pulls = github.open_pull_requests(&self.home).await?;
        let mut tickets = lock(&self.tickets);
        Ok(issues
            .iter()
            .filter(|i| ready(i))
            .filter(|i| !pulls.iter().any(|p| closes(&p.body, i.number)))
            .filter(|i| !tickets.done.contains(&i.number) && tickets.claimed.insert(i.number))
            .map(|i| queue.submit(JobOrigin::Issue, make(i)))
            .collect())
    }

    /// Builds `work` for `ticket` as `agent`. Names, Budget and branch are
    /// checked first; then the files are staged in `sandbox` and every check
    /// must pass there (any failure stops here and opens nothing); then the
    /// files are committed to the Agent's `<agent-name>/<topic>` branch of the
    /// Archodin repository and a pull request opens that links the ticket and
    /// says it was made with AI. If the push half-fails, the branch is deleted
    /// so a retry starts clean. A failed build releases the ticket for `pick_up`.
    pub async fn build<G: GitHub>(
        &self,
        sandbox: &dyn Sandbox,
        github: &ForjaGitHub<G>,
        meter: &UsageMeter,
        agent: &str,
        ticket: &Issue,
        work: ForjaWork,
    ) -> Result<ForjaPullRequest, ForjaError> {
        let result = self
            .build_inner(sandbox, github, meter, agent, ticket, work)
            .await;
        let mut tickets = lock(&self.tickets);
        match &result {
            Ok(_) => {
                tickets.claimed.remove(&ticket.number);
                tickets.done.insert(ticket.number);
            }
            Err(_) => {
                tickets.claimed.remove(&ticket.number);
            }
        }
        result
    }

    async fn build_inner<G: GitHub>(
        &self,
        sandbox: &dyn Sandbox,
        github: &ForjaGitHub<G>,
        meter: &UsageMeter,
        agent: &str,
        ticket: &Issue,
        work: ForjaWork,
    ) -> Result<ForjaPullRequest, ForjaError> {
        if !self.department.agents().iter().any(|a| a == agent) {
            return Err(ForjaError::NotMember(agent.into()));
        }
        let branch = branch_name(agent, &work.topic);
        let slug = branch.split_once('/').map_or("", |(_, s)| s);
        if !valid_part(agent) || !valid_part(slug) {
            return Err(ForjaError::InvalidName(branch));
        }
        if meter.exceeded("", &self.department.budget_key()).is_some() {
            return Err(ForjaError::BudgetExceeded);
        }
        let info = github.repository(&self.home).await?;
        github.allows_branch(&self.home, &branch)?;

        let staged: Vec<(String, String)> = work
            .files
            .iter()
            .map(|(path, content, _)| (path.clone(), content.clone()))
            .collect();
        sandbox.stage(&staged).await?;
        for command in &work.checks {
            let out = sandbox.run(command).await?;
            if out.exit_code != 0 {
                return Err(ForjaError::CheckFailed {
                    command: command.clone(),
                    exit_code: out.exit_code,
                    output: format!("stdout: {}\nstderr: {}", out.stdout, out.stderr),
                });
            }
        }

        github
            .create_branch(&self.home, &branch, &info.default_branch)
            .await?;
        match self.push(github, agent, ticket, &work, &branch, info).await {
            Ok(pull_request) => Ok(ForjaPullRequest {
                agent: agent.into(),
                ticket: ticket.clone(),
                branch,
                title: work.title,
                pull_request,
            }),
            Err(e) => {
                let _ = github.delete_branch(&self.home, &branch).await;
                Err(e)
            }
        }
    }

    async fn push<G: GitHub>(
        &self,
        github: &ForjaGitHub<G>,
        agent: &str,
        ticket: &Issue,
        work: &ForjaWork,
        branch: &str,
        info: RepoInfo,
    ) -> Result<PullRequest, ForjaError> {
        for (path, content, message) in &work.files {
            github
                .commit_file(&self.home, branch, path, content, message)
                .await?;
        }
        let body = format!(
            "{}\n\nTicket: {}\nCloses #{}\n\n---\nDisclosure: this pull request was made with AI by the Archodin Agent `{agent}` of Forja.",
            work.summary, ticket.url, ticket.number
        );
        let draft = PullRequestDraft {
            head: branch.to_string(),
            base: info.default_branch,
            title: work.title.clone(),
            body,
        };
        Ok(github.open_pull_request(&self.home, draft).await?)
    }

    /// Puts the pull request in front of a person through `approver` (the
    /// Agora lists it as pending). Forja only asks: it never merges.
    pub async fn submit_for_review(
        &self,
        approver: &dyn Approver,
        pull_request: &ForjaPullRequest,
    ) -> ApprovalDecision {
        approver.decide(&pull_request.approval_request()).await
    }
}
