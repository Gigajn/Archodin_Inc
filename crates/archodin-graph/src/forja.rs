//! Forja: the Department that develops Archodin (ADR 0006).
//!
//! Forja takes tickets from the Archodin repository, builds in a [`Sandbox`], and
//! opens one pull request per ticket from an `<agent-name>/<topic>` branch. It
//! can never push to `main` or merge its own pull requests: every GitHub call it
//! makes goes through a [`ForjaGitHub`], which refuses them. A pending pull
//! request is put in front of a person as an [`ApprovalRequest`], so it waits in
//! the Agora's approval queue until a person merges it.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use crate::{
    branch_name, ApprovalDecision, ApprovalRequest, Approver, Department, ExploradorError, GitHub,
    GitHubError, Issue, JobId, JobOrigin, JobQueue, PullRequest, PullRequestDraft, RepoInfo,
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

/// A [`GitHub`] that refuses what Forja must never do: write to `main` (or any
/// other protected branch) and merge pull requests. Everything else is passed
/// to the wrapped GitHub.
pub struct ForjaGitHub<G> {
    inner: G,
    home: RepoRef,
    protected: Vec<String>,
}

impl<G: GitHub> ForjaGitHub<G> {
    /// Guards `inner` for Forja working on `home` (the Archodin repository).
    pub fn new(inner: G, home: RepoRef) -> Self {
        Self {
            inner,
            home,
            protected: PROTECTED_BRANCHES.iter().map(|b| b.to_string()).collect(),
        }
    }

    /// Also refuses writes to `branch` (matched ignoring case).
    pub fn protect(mut self, branch: impl Into<String>) -> Self {
        self.protected.push(branch.into());
        self
    }

    /// The wrapped GitHub.
    pub fn inner(&self) -> &G {
        &self.inner
    }

    /// The repository Forja works on.
    pub fn home(&self) -> &RepoRef {
        &self.home
    }

    fn check_branch(&self, repo: &RepoRef, branch: &str) -> Result<(), GitHubError> {
        if self
            .protected
            .iter()
            .any(|p| p.eq_ignore_ascii_case(branch.trim()))
        {
            return Err(forbidden(format!("write to `{branch}` of {repo}")));
        }
        Ok(())
    }

    /// Always refused: only a person merges a pull request.
    pub fn merge_pull_request(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> BoxFuture<'static, Result<(), GitHubError>> {
        let err = forbidden(format!("merge pull request #{number} of {repo}"));
        Box::pin(async move { Err(err) })
    }
}

impl<G: GitHub> GitHub for ForjaGitHub<G> {
    fn repository(&self, repo: &RepoRef) -> BoxFuture<'_, Result<RepoInfo, GitHubError>> {
        self.inner.repository(repo)
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
        match self.check_branch(repo, branch) {
            Ok(()) => self.inner.create_branch(repo, branch, from),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    fn commit_file(
        &self,
        repo: &RepoRef,
        branch: &str,
        path: &str,
        content: &str,
        message: &str,
    ) -> BoxFuture<'_, Result<(), GitHubError>> {
        match self.check_branch(repo, branch) {
            Ok(()) => self.inner.commit_file(repo, branch, path, content, message),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    fn open_pull_request(
        &self,
        upstream: &RepoRef,
        draft: PullRequestDraft,
    ) -> BoxFuture<'_, Result<PullRequest, GitHubError>> {
        self.inner.open_pull_request(upstream, draft)
    }
}

/// Why a Forja operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForjaError {
    /// The Department has no linked repository.
    #[error("the Department has no linked repository")]
    NoRepository,
    /// The Agent does not belong to Forja.
    #[error("`{0}` is not an Agent of Forja")]
    NotMember(String),
    /// A check failed in the Sandbox, so no pull request was opened.
    #[error("check `{command}` failed with exit code {exit_code}: {output}")]
    CheckFailed {
        /// The command that failed.
        command: String,
        /// Its exit code.
        exit_code: i32,
        /// What it printed on standard error.
        output: String,
    },
    /// The Sandbox could not run a check.
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    /// GitHub failed or refused.
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    /// The Department failed.
    #[error(transparent)]
    Department(#[from] ExploradorError),
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

    /// Adds a command that must exit 0 in the Sandbox before the pull request opens.
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

/// The Department that develops Archodin. Clones share state.
#[derive(Clone)]
pub struct Forja {
    department: Department,
    home: RepoRef,
    seen: Arc<Mutex<HashSet<u64>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Forja {
    /// Forja from an approved Department whose Charter points at the Archodin repository.
    pub fn new(department: Department) -> Result<Self, ForjaError> {
        let home = department
            .charter()
            .source()
            .cloned()
            .ok_or(ForjaError::NoRepository)?;
        Ok(Self {
            department,
            home,
            seen: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// The underlying Department.
    pub fn department(&self) -> &Department {
        &self.department
    }

    /// Queues a Job, once each, for every open ticket that `ready` accepts (the
    /// `ready-for-agent` ones), while the Department's Budget lasts. `make`
    /// builds a Job's State from a ticket.
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
        let mut seen = lock(&self.seen);
        Ok(issues
            .iter()
            .filter(|i| ready(i) && seen.insert(i.number))
            .map(|i| queue.submit(JobOrigin::Issue, make(i)))
            .collect())
    }

    /// Builds `work` for `ticket` as `agent`: runs the checks in `sandbox` (any
    /// failure stops here and opens nothing), then commits the files to the
    /// Agent's `<agent-name>/<topic>` branch of the Archodin repository and
    /// opens a pull request that links the ticket and says it was made with AI.
    pub async fn build<G: GitHub>(
        &self,
        sandbox: &dyn Sandbox,
        github: &ForjaGitHub<G>,
        agent: &str,
        ticket: &Issue,
        work: ForjaWork,
    ) -> Result<ForjaPullRequest, ForjaError> {
        if !self.department.agents().iter().any(|a| a == agent) {
            return Err(ForjaError::NotMember(agent.into()));
        }
        for command in &work.checks {
            let out = sandbox.run(command).await?;
            if out.exit_code != 0 {
                return Err(ForjaError::CheckFailed {
                    command: command.clone(),
                    exit_code: out.exit_code,
                    output: out.stderr,
                });
            }
        }
        let info = github.repository(&self.home).await?;
        let branch = branch_name(agent, &work.topic);
        github
            .create_branch(&self.home, &branch, &info.default_branch)
            .await?;
        for (path, content, message) in &work.files {
            github
                .commit_file(&self.home, &branch, path, content, message)
                .await?;
        }
        let body = format!(
            "{}\n\nTicket: {}\nCloses #{}\n\n---\nDisclosure: this pull request was made with AI by the Archodin Agent `{agent}` of Forja.",
            work.summary, ticket.url, ticket.number
        );
        let draft = PullRequestDraft {
            head: branch.clone(),
            base: info.default_branch,
            title: work.title.clone(),
            body,
        };
        let pull_request = github.open_pull_request(&self.home, draft).await?;
        Ok(ForjaPullRequest {
            agent: agent.into(),
            ticket: ticket.clone(),
            branch,
            title: work.title,
            pull_request,
        })
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
