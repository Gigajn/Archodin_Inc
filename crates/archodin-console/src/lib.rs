//! The Agora: the web console where a person submits Jobs, follows them, and
//! approves Skills, scripts, MCP servers, hires and Budget overruns.
//!
//! [`Agora`] implements the core [`Approver`] interface, so any Graph, library or
//! Nakama that asks for approval can be pointed at it. Questions it cannot answer
//! yet come back `Pending` and wait in the approval list; a person answers them
//! through the HTTP API (or the small page at `/`), and a Job paused on such an
//! answer resumes by itself.
//!
//! The Agora does not know what State a Graph carries. The application gives it a
//! runner (see [`AgoraBuilder::runner`]): for each Job it builds and runs its
//! Graph, passing the Agora as the approver and [`pump`]ing the Job events into
//! the [`JobSink`] so they appear live.
//!
//! Routes (see [`Agora::router`]):
//! - `GET /` the page, `GET /approvals/{id}` the same page opened at one approval
//!   (the deep link in notifications)
//! - `GET|POST /login`, `POST /logout` (when [`AgoraBuilder::auth`] is set)
//! - `POST /api/jobs`, `GET /api/jobs`, `GET /api/jobs/{id}`
//! - `GET /api/jobs/{id}/events` server-sent events, replayed from the start
//! - `POST /api/jobs/{id}/cancel`
//! - `GET /api/approvals` pending approvals grouped per Role
//! - `POST /api/approvals/{id}/approve|reject`
//! - `POST /api/roles/{role}/approve-all|reject-all`
//! - `GET /api/usage` token use and cost per Agent and per Job
//!
//! With [`AgoraBuilder::auth`] every route, including unknown paths, needs a login
//! session (cookie or bearer token, see [`Auth`]). Without it the router is open,
//! which is only for tests and trusted local use.
//!
//! With [`AgoraBuilder::notifier`] a new pending approval is sent to the person by
//! Telegram or email, with a link to it; approvals that arrive within the
//! [`notify_window`](AgoraBuilder::notify_window) share one message.

mod auth;
mod notify;

pub use auth::{hash_password, Auth, AuthError};
pub use notify::{EmailNotifier, Mailer, Notification, Notifier, NotifyError, TelegramNotifier};

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use archodin_graph::{
    ApprovalDecision, ApprovalRequest, Approver, JobEvent, JobOrigin, JobQueue, McpLibrary,
    SkillLibrary, Usage, UsageMeter,
};
use archodin_graph::locks::mutex;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use futures::future::BoxFuture;
use futures::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio::task::AbortHandle;

/// A file a person attaches to a Job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// The file name, without any directory.
    pub name: String,
    /// The file's text.
    pub content: String,
}

/// What a person submits as a Job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRequest {
    /// What the person wants done.
    pub request: String,
    /// The Role whose approvals this Job's questions are listed under.
    #[serde(default)]
    pub role: Option<String>,
    /// Links to read (http or https).
    #[serde(default)]
    pub links: Vec<String>,
    /// A GitHub URL the work comes from.
    #[serde(default)]
    pub github_url: Option<String>,
    /// Attached files.
    #[serde(default)]
    pub files: Vec<Attachment>,
}

impl JobRequest {
    fn validate(&self) -> Result<(), String> {
        if self.request.trim().is_empty() {
            return Err("the request is empty".into());
        }
        if let Some(bad) = self
            .links
            .iter()
            .find(|l| !(l.starts_with("https://") || l.starts_with("http://")))
        {
            return Err(format!("`{bad}` is not an http or https link"));
        }
        if let Some(url) = &self.github_url {
            if !url.starts_with("https://github.com/") {
                return Err(format!("`{url}` is not a GitHub URL"));
            }
        }
        if let Some(f) = self
            .files
            .iter()
            .find(|f| f.name.is_empty() || f.name.contains(['/', '\\']) || f.name == "..")
        {
            return Err(format!("`{}` is not a plain file name", f.name));
        }
        Ok(())
    }
}

/// One thing that happened during a Job, as shown live in the Agora. The State is
/// shown with `Debug`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobUpdate {
    /// The Job began.
    JobStarted,
    /// A Node began executing.
    NodeStarted { node: String },
    /// A Node finished.
    NodeFinished { node: String },
    /// Parallel Nodes wrote competing values.
    Conflict { field: String, nodes: Vec<String> },
    /// The Qadi settled a conflict.
    Decision {
        field: String,
        node: String,
        value: String,
    },
    /// The Job ended with this State.
    JobFinished { state: String },
    /// The Job waits for a person's approval.
    JobPaused {
        node: String,
        prompt: String,
        state: String,
    },
    /// A person rejected a step; the Job stopped.
    JobRejected { node: String, state: String },
    /// The Job hit its Step limit.
    StepLimitReached { limit: usize, state: String },
    /// A person cancelled the Job.
    Cancelled,
    /// An answer arrived and the Job continues.
    Resumed,
}

impl JobUpdate {
    /// Converts a Graph's [`JobEvent`].
    pub fn from_event<S: Debug>(event: &JobEvent<S>) -> Self {
        match event {
            JobEvent::JobStarted => Self::JobStarted,
            JobEvent::NodeStarted { node } => Self::NodeStarted { node: node.clone() },
            JobEvent::NodeFinished { node } => Self::NodeFinished { node: node.clone() },
            JobEvent::Conflict { field, nodes } => Self::Conflict {
                field: field.clone(),
                nodes: nodes.clone(),
            },
            JobEvent::Decision { field, node, value } => Self::Decision {
                field: field.clone(),
                node: node.clone(),
                value: value.clone(),
            },
            JobEvent::JobFinished { state } => Self::JobFinished {
                state: format!("{state:?}"),
            },
            JobEvent::JobPaused {
                node,
                prompt,
                state,
            } => Self::JobPaused {
                node: node.clone(),
                prompt: prompt.clone(),
                state: format!("{state:?}"),
            },
            JobEvent::JobRejected { node, state } => Self::JobRejected {
                node: node.clone(),
                state: format!("{state:?}"),
            },
            JobEvent::StepLimitReached { limit, state } => Self::StepLimitReached {
                limit: *limit,
                state: format!("{state:?}"),
            },
        }
    }

    fn name(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v["type"].as_str().map(String::from))
            .unwrap_or_default()
    }
}

/// Feeds a running Job's events to the Agora.
#[derive(Clone)]
pub struct JobSink {
    agora: Agora,
    id: u64,
}

impl JobSink {
    /// Records an update, shown live to anyone following the Job.
    pub fn emit(&self, update: JobUpdate) {
        self.agora.emit(self.id, update);
    }
}

/// Sends every event of a Graph's stream to `sink`.
pub async fn pump<S: Debug>(mut events: impl Stream<Item = JobEvent<S>> + Unpin, sink: &JobSink) {
    while let Some(event) = events.next().await {
        sink.emit(JobUpdate::from_event(&event));
    }
}

/// What the runner gets for one Job.
pub struct JobContext {
    /// The Job's id as Graphs, meters and approvals know it (`job-<n>`). Running
    /// the same id again resumes a durable Job from its Checkpoint.
    pub job: String,
    /// What the person submitted.
    pub request: JobRequest,
    /// The Agora, to pass as the [`Approver`] (wrap it in `BudgetApprover` to meter).
    pub agora: Agora,
    /// Where the Job's events go.
    pub sink: JobSink,
}

type Runner = Arc<dyn Fn(JobContext) -> BoxFuture<'static, ()> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Queued,
    Running,
    Paused,
    Finished,
    Rejected,
    Failed,
    Cancelled,
}

impl Status {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Finished | Self::Rejected | Self::Failed | Self::Cancelled
        )
    }
}

struct JobRecord {
    request: JobRequest,
    status: Status,
    events: Vec<JobUpdate>,
    handle: Option<AbortHandle>,
    generation: u64,
}

type Key = (String, String, String);

struct Approval {
    id: u64,
    key: Key,
    role: String,
    kind: &'static str,
}

struct State_ {
    jobs: BTreeMap<u64, JobRecord>,
    next_job: u64,
    approvals: BTreeMap<u64, Approval>,
    next_approval: u64,
    decided: HashMap<Key, ApprovalDecision>,
}

struct Inner {
    state: Mutex<State_>,
    queue: JobQueue<u64>,
    changed: watch::Sender<u64>,
    runner: Runner,
    meter: Arc<UsageMeter>,
    skills: Option<SkillLibrary>,
    mcp: Option<McpLibrary>,
    agents: Vec<String>,
    max_running: usize,
    auth: Option<Auth>,
    notifiers: Vec<Arc<dyn Notifier>>,
    notify_window: Duration,
    public_url: String,
    notes: Mutex<Notes>,
}

/// Approvals waiting to be announced, and the last notification failure.
#[derive(Default)]
struct Notes {
    batch: Vec<u64>,
    scheduled: bool,
    last_error: Option<String>,
}

/// The Agora. Clones share the same console.
#[derive(Clone)]
pub struct Agora {
    inner: Arc<Inner>,
}

/// Builds an [`Agora`].
pub struct AgoraBuilder {
    runner: Option<Runner>,
    meter: Arc<UsageMeter>,
    skills: Option<SkillLibrary>,
    mcp: Option<McpLibrary>,
    agents: Vec<String>,
    max_running: usize,
    auth: Option<Auth>,
    notifiers: Vec<Arc<dyn Notifier>>,
    notify_window: Duration,
    public_url: String,
}

impl AgoraBuilder {
    /// Sets how a Job is run: build the Graph for `ctx.request`, run it with
    /// `ctx.agora` as approver, and [`pump`] its events into `ctx.sink`.
    pub fn runner<F>(mut self, runner: F) -> Self
    where
        F: Fn(JobContext) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    {
        self.runner = Some(Arc::new(runner));
        self
    }

    /// The meter whose token use and cost the Agora shows.
    pub fn meter(mut self, meter: Arc<UsageMeter>) -> Self {
        self.meter = meter;
        self
    }

    /// The Skill library whose pending Skills (and scripts) the Agora lists.
    pub fn skills(mut self, skills: SkillLibrary) -> Self {
        self.skills = Some(skills);
        self
    }

    /// The MCP library whose pending servers the Agora lists.
    pub fn mcp(mut self, mcp: McpLibrary) -> Self {
        self.mcp = Some(mcp);
        self
    }

    /// The Agents whose usage is shown.
    pub fn agents<N: Into<String>>(mut self, names: impl IntoIterator<Item = N>) -> Self {
        self.agents = names.into_iter().map(Into::into).collect();
        self
    }

    /// The most Jobs running at once; the rest wait in the queue (default 4).
    pub fn max_running(mut self, n: usize) -> Self {
        self.max_running = n.max(1);
        self
    }

    /// Requires a login session on every page and API call.
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Adds a way to notify the person of new pending approvals. Add several (for
    /// example Telegram and email) and each gets every notification.
    pub fn notifier(mut self, notifier: Arc<dyn Notifier>) -> Self {
        self.notifiers.push(notifier);
        self
    }

    /// How long to wait after the first new approval before notifying, so a burst
    /// becomes one message (default 30 seconds).
    pub fn notify_window(mut self, window: Duration) -> Self {
        self.notify_window = window;
        self
    }

    /// The address people reach the Agora at, used for links in notifications. Leave
    /// it out and the links are paths such as `/approvals/3`.
    pub fn public_url(mut self, url: impl Into<String>) -> Self {
        self.public_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// Builds the Agora.
    ///
    /// # Panics
    /// Panics if no runner was set.
    pub fn build(self) -> Agora {
        Agora {
            inner: Arc::new(Inner {
                state: Mutex::new(State_ {
                    jobs: BTreeMap::new(),
                    next_job: 1,
                    approvals: BTreeMap::new(),
                    next_approval: 1,
                    decided: HashMap::new(),
                }),
                queue: JobQueue::new(),
                changed: watch::channel(0).0,
                runner: self.runner.expect("the Agora needs a runner"),
                meter: self.meter,
                skills: self.skills,
                mcp: self.mcp,
                agents: self.agents,
                max_running: self.max_running,
                auth: self.auth,
                notifiers: self.notifiers,
                notify_window: self.notify_window,
                public_url: self.public_url,
                notes: Mutex::new(Notes::default()),
            }),
        }
    }
}

fn job_name(id: u64) -> String {
    format!("job-{id}")
}

fn job_number(name: &str) -> Option<u64> {
    name.strip_prefix("job-")?.parse().ok()
}

impl Agora {
    /// Starts building an Agora.
    pub fn builder() -> AgoraBuilder {
        AgoraBuilder {
            runner: None,
            meter: Arc::new(UsageMeter::new()),
            skills: None,
            mcp: None,
            agents: Vec::new(),
            max_running: 4,
            auth: None,
            notifiers: Vec::new(),
            notify_window: Duration::from_secs(30),
            public_url: String::new(),
        }
    }

    fn bump(&self) {
        self.inner.changed.send_modify(|v| *v += 1);
    }

    fn emit(&self, id: u64, update: JobUpdate) {
        {
            let mut s = mutex(&self.inner.state);
            let Some(job) = s.jobs.get_mut(&id) else {
                return;
            };
            if job.status == Status::Cancelled {
                return;
            }
            match &update {
                JobUpdate::JobFinished { .. } => job.status = Status::Finished,
                JobUpdate::JobPaused { .. } => job.status = Status::Paused,
                JobUpdate::JobRejected { .. } => job.status = Status::Rejected,
                JobUpdate::StepLimitReached { .. } => job.status = Status::Failed,
                _ => {}
            }
            job.events.push(update);
        }
        self.bump();
        self.start_queued();
    }

    /// Submits a Job: it is queued (Agora Jobs first) and started when there is room.
    pub fn submit(&self, request: JobRequest) -> Result<u64, String> {
        request.validate()?;
        let id = {
            let mut s = mutex(&self.inner.state);
            let id = s.next_job;
            s.next_job += 1;
            s.jobs.insert(
                id,
                JobRecord {
                    request,
                    status: Status::Queued,
                    events: Vec::new(),
                    handle: None,
                    generation: 0,
                },
            );
            id
        };
        self.inner.queue.submit(JobOrigin::Agora, id);
        self.bump();
        self.start_queued();
        Ok(id)
    }

    fn start_queued(&self) {
        loop {
            let id = {
                let s = mutex(&self.inner.state);
                let running = s
                    .jobs
                    .values()
                    .filter(|j| j.status == Status::Running)
                    .count();
                if running >= self.inner.max_running {
                    return;
                }
                drop(s);
                match self.inner.queue.next() {
                    Some(q) => q.state,
                    None => return,
                }
            };
            let queued = mutex(&self.inner.state)
                .jobs
                .get(&id)
                .is_some_and(|j| j.status == Status::Queued);
            if queued {
                self.spawn(id, false);
            }
        }
    }

    fn spawn(&self, id: u64, resumed: bool) {
        let (request, generation) = {
            let mut s = mutex(&self.inner.state);
            let Some(job) = s.jobs.get_mut(&id) else {
                return;
            };
            job.status = Status::Running;
            job.generation += 1;
            if resumed {
                job.events.push(JobUpdate::Resumed);
            }
            (job.request.clone(), job.generation)
        };
        let ctx = JobContext {
            job: job_name(id),
            request,
            agora: self.clone(),
            sink: JobSink {
                agora: self.clone(),
                id,
            },
        };
        let run = (self.inner.runner)(ctx);
        let agora = self.clone();
        let task = tokio::spawn(async move {
            run.await;
            agora.runner_ended(id, generation);
        });
        if let Some(job) = mutex(&self.inner.state).jobs.get_mut(&id) {
            job.handle = Some(task.abort_handle());
        }
        self.bump();
    }

    fn runner_ended(&self, id: u64, generation: u64) {
        {
            let mut s = mutex(&self.inner.state);
            if let Some(job) = s.jobs.get_mut(&id) {
                if job.generation == generation && job.status == Status::Running {
                    job.status = Status::Finished;
                }
            }
        }
        self.bump();
        self.start_queued();
    }

    /// Cancels a queued, running or paused Job. Returns false if there is no such
    /// Job or it already ended.
    pub fn cancel(&self, id: u64) -> bool {
        {
            let mut s = mutex(&self.inner.state);
            let Some(job) = s.jobs.get_mut(&id) else {
                return false;
            };
            if job.status.is_terminal() {
                return false;
            }
            if let Some(handle) = job.handle.take() {
                handle.abort();
            }
            job.status = Status::Cancelled;
            job.events.push(JobUpdate::Cancelled);
            let name = job_name(id);
            let gone: Vec<u64> = s
                .approvals
                .values()
                .filter(|a| a.key.0 == name)
                .map(|a| a.id)
                .collect();
            for a in gone {
                s.approvals.remove(&a);
            }
        }
        self.bump();
        self.start_queued();
        true
    }

    fn classify(&self, request: &ApprovalRequest) -> (String, &'static str) {
        let job = request.job.as_str();
        if let Some(role) = job.strip_prefix("skills/") {
            let kind = if request.prompt.starts_with("Approve the scripts") {
                "scripts"
            } else {
                "skill"
            };
            return (role.to_string(), kind);
        }
        if let Some(role) = job.strip_prefix("mcp/") {
            return (role.to_string(), "mcp_server");
        }
        if job == "forja" {
            return ("forja".into(), "pull_request");
        }
        if job == "nakama" {
            return ("nakama".into(), "nakama_proposal");
        }
        let role = job_number(job)
            .and_then(|n| {
                mutex(&self.inner.state)
                    .jobs
                    .get(&n)
                    .and_then(|j| j.request.role.clone())
            })
            .unwrap_or_else(|| "unassigned".into());
        let kind = if request.prompt.contains("exceeded its Budget") {
            "budget_overrun"
        } else {
            "step"
        };
        (role, kind)
    }

    /// Asks the libraries about everything pending so it shows up in the list, and
    /// applies decisions a person has already made.
    async fn sync(&self) {
        if let Some(skills) = &self.inner.skills {
            skills.review_pending(self).await;
        }
        if let Some(mcp) = &self.inner.mcp {
            mcp.review_pending(self).await;
        }
        mutex(&self.inner.state)
            .decided
            .retain(|k, _| !(k.0.starts_with("skills/") || k.0.starts_with("mcp/")));
    }

    fn summary_of(&self, role: &str, skill: &str) -> (Option<String>, bool) {
        self.inner
            .skills
            .as_ref()
            .and_then(|lib| {
                lib.pending()
                    .into_iter()
                    .find(|p| p.role == role && p.skill.name() == skill)
            })
            .map(|p| (p.summary, p.unverified))
            .unwrap_or((None, false))
    }

    async fn approvals_view(&self) -> Value {
        self.sync().await;
        let items: Vec<(u64, Key, String, &'static str)> = mutex(&self.inner.state)
            .approvals
            .values()
            .map(|a| (a.id, a.key.clone(), a.role.clone(), a.kind))
            .collect();
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (id, (job, node, prompt), role, kind) in items {
            let (summary, unverified) = if kind == "skill" {
                self.summary_of(&role, &node)
            } else {
                (None, false)
            };
            groups.entry(role).or_default().push(json!({
                "id": id, "kind": kind, "job": job, "node": node, "prompt": prompt,
                "summary": summary, "unverified": unverified,
            }));
        }
        Value::Array(
            groups
                .into_iter()
                .map(|(role, items)| json!({"role": role, "items": items}))
                .collect(),
        )
    }

    /// Answers one pending approval. Returns false if there is none with this id.
    pub async fn decide_approval(&self, id: u64, decision: ApprovalDecision) -> bool {
        let found = {
            let mut s = mutex(&self.inner.state);
            match s.approvals.remove(&id) {
                Some(a) => {
                    s.decided.insert(a.key, decision);
                    true
                }
                None => false,
            }
        };
        if found {
            self.after_decisions().await;
        }
        found
    }

    /// Answers every pending approval of a Role; returns how many.
    pub async fn decide_role(&self, role: &str, decision: ApprovalDecision) -> usize {
        self.sync().await;
        let n = {
            let mut s = mutex(&self.inner.state);
            let ids: Vec<u64> = s
                .approvals
                .values()
                .filter(|a| a.role == role)
                .map(|a| a.id)
                .collect();
            for id in &ids {
                if let Some(a) = s.approvals.remove(id) {
                    s.decided.insert(a.key, decision);
                }
            }
            ids.len()
        };
        self.after_decisions().await;
        n
    }

    async fn after_decisions(&self) {
        self.sync().await;
        let paused: Vec<u64> = {
            let s = mutex(&self.inner.state);
            s.decided
                .keys()
                .filter_map(|k| job_number(&k.0))
                .filter(|n| s.jobs.get(n).is_some_and(|j| j.status == Status::Paused))
                .collect()
        };
        for id in paused {
            self.spawn(id, true);
        }
        self.bump();
    }

    /// The axum router serving the page and the API.
    pub fn router(&self) -> Router {
        let router = Router::new()
            .route("/", get(index))
            .route("/approvals/{id}", get(index))
            .route("/api/jobs", post(submit_job).get(list_jobs))
            .route("/api/jobs/{id}", get(get_job))
            .route("/api/jobs/{id}/events", get(job_events))
            .route("/api/jobs/{id}/cancel", post(cancel_job))
            .route("/api/approvals", get(list_approvals))
            .route("/api/approvals/{id}/approve", post(approve_one))
            .route("/api/approvals/{id}/reject", post(reject_one))
            .route("/api/roles/{role}/approve-all", post(approve_role))
            .route("/api/roles/{role}/reject-all", post(reject_role))
            .route("/api/usage", get(usage))
            .with_state(self.clone());
        match &self.inner.auth {
            Some(auth) => auth.protect(router),
            None => router,
        }
    }

    /// The last reason a notification failed to send, if any did.
    pub fn last_notification_error(&self) -> Option<String> {
        mutex(&self.inner.notes).last_error.clone()
    }

    /// Asks the Skill and MCP libraries about everything pending, so new ones are
    /// listed (and announced) without waiting for someone to open the page. Call it
    /// now and then from the application.
    pub async fn refresh(&self) {
        self.sync().await;
    }

    fn announce(&self, id: u64) {
        if self.inner.notifiers.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        {
            let mut notes = mutex(&self.inner.notes);
            notes.batch.push(id);
            if notes.scheduled {
                return;
            }
            notes.scheduled = true;
        }
        let agora = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(agora.inner.notify_window).await;
            agora.flush_notifications().await;
        });
    }

    async fn flush_notifications(&self) {
        let ids = {
            let mut notes = mutex(&self.inner.notes);
            notes.scheduled = false;
            std::mem::take(&mut notes.batch)
        };
        // Skip what was answered or cancelled while the window was open.
        let waiting: Vec<(u64, &'static str, String, String)> = {
            let s = mutex(&self.inner.state);
            ids.iter()
                .filter_map(|id| s.approvals.get(id))
                .map(|a| (a.id, a.kind, a.role.clone(), a.key.2.clone()))
                .collect()
        };
        if waiting.is_empty() {
            return;
        }
        let notification = self.compose(&waiting);
        for notifier in &self.inner.notifiers {
            if let Err(e) = notifier.send(&notification).await {
                mutex(&self.inner.notes).last_error = Some(e.to_string());
            }
        }
    }

    fn compose(&self, waiting: &[(u64, &'static str, String, String)]) -> Notification {
        const SHOWN: usize = 10;
        let base = &self.inner.public_url;
        let subject = match waiting {
            [(_, kind, role, _)] => format!("Approval waiting: {kind} for {role}"),
            _ => format!("{} approvals waiting", waiting.len()),
        };
        let mut body = String::new();
        for (id, kind, role, prompt) in waiting.iter().take(SHOWN) {
            let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
            let prompt: String = if prompt.chars().count() > 160 {
                prompt.chars().take(160).chain("...".chars()).collect()
            } else {
                prompt
            };
            body.push_str(&format!(
                "- [{kind}, {role}] {prompt}\n  {base}/approvals/{id}\n"
            ));
        }
        if waiting.len() > SHOWN {
            body.push_str(&format!("...and {} more: {base}/\n", waiting.len() - SHOWN));
        }
        Notification { subject, body }
    }
}

impl Approver for Agora {
    fn decide(&self, request: &ApprovalRequest) -> BoxFuture<'_, ApprovalDecision> {
        let key: Key = (
            request.job.clone(),
            request.node.clone(),
            request.prompt.clone(),
        );
        let (role, kind) = self.classify(request);
        let mut new_approval = None;
        let decision = {
            let mut s = mutex(&self.inner.state);
            if let Some(d) = s.decided.remove(&key) {
                d
            } else {
                if !s.approvals.values().any(|a| a.key == key) {
                    let id = s.next_approval;
                    s.next_approval += 1;
                    s.approvals.insert(
                        id,
                        Approval {
                            id,
                            key,
                            role,
                            kind,
                        },
                    );
                    new_approval = Some(id);
                }
                ApprovalDecision::Pending
            }
        };
        if let Some(id) = new_approval {
            self.announce(id);
        }
        if decision == ApprovalDecision::Pending {
            self.bump();
        }
        Box::pin(async move { decision })
    }
}

type ApiError = (StatusCode, String);

fn not_found(what: &str) -> ApiError {
    (StatusCode::NOT_FOUND, format!("no such {what}"))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

fn job_view(id: u64, job: &JobRecord, meter: &UsageMeter) -> Value {
    let used = meter.job_usage(&job_name(id));
    json!({
        "id": id,
        "job": job_name(id),
        "status": job.status,
        "request": job.request.request,
        "role": job.request.role,
        "links": job.request.links,
        "github_url": job.request.github_url,
        "files": job.request.files.iter().map(|f| &f.name).collect::<Vec<_>>(),
        "events": job.events.len(),
        "total_tokens": used.total_tokens(),
        "cost": used.cost,
    })
}

async fn submit_job(
    State(agora): State<Agora>,
    Json(request): Json<JobRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = agora
        .submit(request)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))))
}

async fn list_jobs(State(agora): State<Agora>) -> Json<Value> {
    let s = mutex(&agora.inner.state);
    Json(Value::Array(
        s.jobs
            .iter()
            .map(|(id, j)| job_view(*id, j, &agora.inner.meter))
            .collect(),
    ))
}

async fn get_job(State(agora): State<Agora>, Path(id): Path<u64>) -> Result<Json<Value>, ApiError> {
    let s = mutex(&agora.inner.state);
    let job = s.jobs.get(&id).ok_or_else(|| not_found("Job"))?;
    Ok(Json(job_view(id, job, &agora.inner.meter)))
}

async fn cancel_job(
    State(agora): State<Agora>,
    Path(id): Path<u64>,
) -> Result<Json<Value>, ApiError> {
    if !mutex(&agora.inner.state).jobs.contains_key(&id) {
        return Err(not_found("Job"));
    }
    Ok(Json(json!({"cancelled": agora.cancel(id)})))
}

async fn job_events(State(agora): State<Agora>, Path(id): Path<u64>) -> Result<Response, ApiError> {
    if !mutex(&agora.inner.state).jobs.contains_key(&id) {
        return Err(not_found("Job"));
    }
    let rx = agora.inner.changed.subscribe();
    let events = stream::unfold(
        (agora, rx, 0usize),
        move |(agora, mut rx, idx)| async move {
            loop {
                let next = {
                    let s = mutex(&agora.inner.state);
                    let job = s.jobs.get(&id)?;
                    match job.events.get(idx) {
                        Some(u) => Some(Some(u.clone())),
                        None if job.status.is_terminal() => return None,
                        None => None,
                    }
                };
                match next.flatten() {
                    Some(update) => {
                        let event = Event::default()
                            .event(update.name())
                            .json_data(&update)
                            .unwrap_or_default();
                        return Some((Ok::<_, Infallible>(event), (agora, rx, idx + 1)));
                    }
                    None => {
                        if rx.changed().await.is_err() {
                            return None;
                        }
                    }
                }
            }
        },
    );
    Ok(Sse::new(events)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response())
}

async fn list_approvals(State(agora): State<Agora>) -> Json<Value> {
    Json(agora.approvals_view().await)
}

async fn answer_one(agora: Agora, id: u64, d: ApprovalDecision) -> Result<Json<Value>, ApiError> {
    agora.approvals_view().await;
    if agora.decide_approval(id, d).await {
        Ok(Json(json!({"ok": true})))
    } else {
        Err(not_found("approval"))
    }
}

async fn approve_one(State(a): State<Agora>, Path(id): Path<u64>) -> Result<Json<Value>, ApiError> {
    answer_one(a, id, ApprovalDecision::Approved).await
}

async fn reject_one(State(a): State<Agora>, Path(id): Path<u64>) -> Result<Json<Value>, ApiError> {
    answer_one(a, id, ApprovalDecision::Rejected).await
}

async fn approve_role(State(a): State<Agora>, Path(role): Path<String>) -> Json<Value> {
    Json(json!({"decided": a.decide_role(&role, ApprovalDecision::Approved).await}))
}

async fn reject_role(State(a): State<Agora>, Path(role): Path<String>) -> Json<Value> {
    Json(json!({"decided": a.decide_role(&role, ApprovalDecision::Rejected).await}))
}

fn usage_json(extra: (&str, Value), u: Usage) -> Value {
    json!({
        extra.0: extra.1,
        "prompt_tokens": u.prompt_tokens,
        "completion_tokens": u.completion_tokens,
        "total_tokens": u.total_tokens(),
        "cost": u.cost,
    })
}

async fn usage(State(agora): State<Agora>) -> Json<Value> {
    let meter = &agora.inner.meter;
    let agents: Vec<Value> = agora
        .inner
        .agents
        .iter()
        .map(|n| usage_json(("name", json!(n)), meter.agent_usage(n)))
        .collect();
    let jobs: Vec<Value> = mutex(&agora.inner.state)
        .jobs
        .keys()
        .map(|id| usage_json(("id", json!(id)), meter.job_usage(&job_name(*id))))
        .collect();
    Json(json!({"agents": agents, "jobs": jobs}))
}
