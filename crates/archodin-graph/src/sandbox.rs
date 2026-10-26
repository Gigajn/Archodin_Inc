//! The Sandbox: an isolated place where Agents run code, tests and approved Skill
//! scripts without touching the host machine (issue #26, spec #2, ADR 0004).
//!
//! [`Sandbox`] is a trait so Jobs do not depend on how isolation is done.
//! [`ContainerSandbox`] runs each command in a throwaway container through the
//! docker or podman CLI: no network unless the [`SandboxPolicy`] allows it, CPU,
//! memory and time limits, a read-only root, no host mounts and no inherited
//! environment (so no host files or secrets). [`ScriptedSandbox`] is a fake for
//! tests. [`SandboxTool`] lets an Agent call a Sandbox as a Tool, and
//! [`run_skill_script`] is the only way to run an approved Skill script: it takes
//! a Sandbox, so scripts never run on the host.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::{SkillError, SkillLibrary, TypedTool};

/// Which limit of a Sandbox a command ran into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Limit {
    /// The command ran longer than the time limit and was killed.
    #[error("time")]
    Time,
    /// The command used more memory than allowed and was killed.
    #[error("memory")]
    Memory,
    /// The command tried to use a network the Sandbox does not have.
    #[error("network")]
    Network,
}

/// Why a command did not complete in a Sandbox.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    /// The command was stopped by a limit.
    #[error("sandbox {0} limit hit")]
    LimitHit(Limit),
    /// The container runtime is not installed or not reachable; nothing ran.
    #[error("container runtime unavailable: {0}")]
    RuntimeUnavailable(String),
    /// The runtime failed to start the container.
    #[error("container runtime error: {0}")]
    Runtime(String),
}

/// What a command printed and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxOutput {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// The exit code; non-zero means the command itself failed.
    pub exit_code: i32,
}

/// The limits and access a Sandbox gives a command. The default has no network.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxPolicy {
    image: String,
    cpus: f64,
    memory_mb: u64,
    timeout: Duration,
    network: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            image: "alpine:3".into(),
            cpus: 1.0,
            memory_mb: 256,
            timeout: Duration::from_secs(60),
            network: false,
        }
    }
}

impl SandboxPolicy {
    /// The container image commands run in.
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }
    /// The CPU limit, in cores.
    pub fn with_cpus(mut self, cpus: f64) -> Self {
        self.cpus = cpus;
        self
    }
    /// The memory limit, in MiB (swap is not allowed beyond it).
    pub fn with_memory_mb(mut self, memory_mb: u64) -> Self {
        self.memory_mb = memory_mb;
        self
    }
    /// How long a command may run before it is killed.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    /// Whether the command gets network access (off by default).
    pub fn with_network(mut self, network: bool) -> Self {
        self.network = network;
        self
    }
    /// The container image.
    pub fn image(&self) -> &str {
        &self.image
    }
    /// The CPU limit, in cores.
    pub fn cpus(&self) -> f64 {
        self.cpus
    }
    /// The memory limit, in MiB.
    pub fn memory_mb(&self) -> u64 {
        self.memory_mb
    }
    /// The time limit.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Whether the network is on.
    pub fn network(&self) -> bool {
        self.network
    }
}

/// An isolated place that runs a shell command and returns its output.
pub trait Sandbox: Send + Sync {
    /// Runs `command` with `sh -c` inside the Sandbox.
    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, Result<SandboxOutput, SandboxError>>;

    /// Puts `files` (relative path and content) in the Sandbox's working
    /// directory so later commands can see them, replacing earlier staged files.
    /// Sandboxes that cannot stage files refuse.
    fn stage<'a>(
        &'a self,
        files: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<(), SandboxError>> {
        let _ = files;
        Box::pin(async {
            Err(SandboxError::Runtime(
                "this Sandbox cannot stage files".into(),
            ))
        })
    }
}

/// A throwaway host directory holding staged files, removed when dropped.
#[derive(Debug)]
struct StageDir(std::path::PathBuf);

impl Drop for StageDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_staged(files: &[(String, String)]) -> Result<StageDir, SandboxError> {
    use std::path::{Component, Path};
    let fail = |e: std::io::Error| SandboxError::Runtime(format!("staging files: {e}"));
    let dir = StageDir(std::env::temp_dir().join(format!(
        "archodin-stage-{}-{}",
        std::process::id(),
        CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&dir.0).map_err(fail)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The container runs as an unprivileged user that must write here.
        std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o777)).map_err(fail)?;
    }
    for (path, content) in files {
        let relative = Path::new(path);
        let safe = !path.is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
        if !safe {
            return Err(SandboxError::Runtime(format!(
                "cannot stage `{path}`: the path must be relative and stay inside the Sandbox"
            )));
        }
        let target = dir.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut p = parent;
                while p.starts_with(&dir.0) && p != dir.0 {
                    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o777))
                        .map_err(fail)?;
                    p = p.parent().expect("inside the stage directory");
                }
            }
        }
        std::fs::write(&target, content).map_err(fail)?;
    }
    Ok(dir)
}

static CONTAINER_SEQ: AtomicU64 = AtomicU64::new(0);
const MAX_STREAM_BYTES: usize = 64 * 1024;

/// A Sandbox that runs each command in a throwaway docker or podman container.
#[derive(Debug, Clone)]
pub struct ContainerSandbox {
    runtime: String,
    policy: SandboxPolicy,
    staged: Arc<Mutex<Option<Arc<StageDir>>>>,
}

impl ContainerSandbox {
    /// Uses the `docker` CLI.
    pub fn docker(policy: SandboxPolicy) -> Self {
        Self::with_runtime("docker", policy)
    }
    /// Uses the `podman` CLI.
    pub fn podman(policy: SandboxPolicy) -> Self {
        Self::with_runtime("podman", policy)
    }
    /// Uses any docker-compatible CLI by name or path.
    pub fn with_runtime(runtime: impl Into<String>, policy: SandboxPolicy) -> Self {
        Self {
            runtime: runtime.into(),
            policy,
            staged: Arc::new(Mutex::new(None)),
        }
    }
    /// The runtime CLI in use.
    pub fn runtime(&self) -> &str {
        &self.runtime
    }

    /// The arguments given to the runtime to run `command` in a container
    /// called `name`. There are no environment variables, and no mounts except
    /// the throwaway directory of files given to [`Sandbox::stage`] (at `/work`,
    /// which is where commands start): never host files or secrets.
    pub fn run_args(&self, name: &str, command: &str) -> Vec<String> {
        let p = &self.policy;
        let mem = format!("{}m", p.memory_mb);
        let mut args: Vec<String> = ["run", "--rm", "--name", name].map(String::from).into();
        if !p.network {
            args.extend(["--network", "none"].map(String::from));
        }
        args.extend(["--cpus".into(), p.cpus.to_string()]);
        args.extend(["--memory".into(), mem.clone(), "--memory-swap".into(), mem]);
        args.extend(
            [
                "--pids-limit",
                "256",
                "--read-only",
                "--tmpfs",
                "/tmp:rw,size=64m",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--user",
                "65534:65534",
            ]
            .map(String::from),
        );
        if let Some(dir) = self
            .staged
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            args.extend([
                "--volume".to_string(),
                format!("{}:/work:rw", dir.0.display()),
                "--workdir".to_string(),
                "/work".to_string(),
            ]);
        }
        args.push(p.image.clone());
        args.extend(["sh", "-c", command].map(String::from));
        args
    }

    async fn kill(&self, name: &str) {
        let _ = Command::new(&self.runtime)
            .args(["rm", "-f", name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

async fn read_capped(mut r: impl tokio::io::AsyncRead + Unpin) -> String {
    let mut buf = Vec::new();
    let _ = (&mut r)
        .take(MAX_STREAM_BYTES as u64)
        .read_to_end(&mut buf)
        .await;
    // Keep draining so the container is never blocked on a full pipe.
    let _ = tokio::io::copy(&mut r, &mut tokio::io::sink()).await;
    String::from_utf8_lossy(&buf).into_owned()
}

impl Sandbox for ContainerSandbox {
    fn stage<'a>(
        &'a self,
        files: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<(), SandboxError>> {
        Box::pin(async move {
            let dir = write_staged(files)?;
            *self.staged.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(dir));
            Ok(())
        })
    }

    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(async move {
            let name = format!(
                "archodin-sandbox-{}-{}",
                std::process::id(),
                CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed)
            );
            let mut child = Command::new(&self.runtime)
                .args(self.run_args(&name, command))
                .env_clear()
                .envs(std::env::var_os("PATH").map(|p| ("PATH", p)))
                .envs(std::env::var_os("HOME").map(|p| ("HOME", p)))
                .envs(std::env::var_os("XDG_RUNTIME_DIR").map(|p| ("XDG_RUNTIME_DIR", p)))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| SandboxError::RuntimeUnavailable(format!("{}: {e}", self.runtime)))?;
            let out = child.stdout.take().expect("stdout is piped");
            let err = child.stderr.take().expect("stderr is piped");
            let work = async {
                let (stdout, stderr, status) =
                    futures::join!(read_capped(out), read_capped(err), child.wait());
                (stdout, stderr, status)
            };
            let Ok((stdout, stderr, status)) =
                tokio::time::timeout(self.policy.timeout, work).await
            else {
                self.kill(&name).await;
                return Err(SandboxError::LimitHit(Limit::Time));
            };
            let code = status
                .map_err(|e| SandboxError::Runtime(e.to_string()))?
                .code()
                .unwrap_or(137);
            match code {
                // The runtime itself failed (daemon down, image missing, bad flags).
                125 => Err(SandboxError::Runtime(stderr.trim().to_string())),
                // Killed by SIGKILL without our timeout: the memory limit.
                137 => Err(SandboxError::LimitHit(Limit::Memory)),
                exit_code => Ok(SandboxOutput {
                    stdout,
                    stderr,
                    exit_code,
                }),
            }
        })
    }
}

/// A fake Sandbox for tests: replays scripted results in order and records the
/// commands it was asked to run.
pub struct ScriptedSandbox {
    results: Mutex<VecDeque<Result<SandboxOutput, SandboxError>>>,
    commands: Mutex<Vec<String>>,
    staged: Mutex<Vec<(String, String)>>,
}

impl ScriptedSandbox {
    /// A fake that answers the first command with the first result, and so on.
    pub fn new(results: impl IntoIterator<Item = Result<SandboxOutput, SandboxError>>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            commands: Mutex::new(Vec::new()),
            staged: Mutex::new(Vec::new()),
        }
    }
    /// The files staged so far.
    pub fn staged(&self) -> Vec<(String, String)> {
        self.staged
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    /// The commands run so far, in order.
    pub fn commands(&self) -> Vec<String> {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Sandbox for ScriptedSandbox {
    fn stage<'a>(
        &'a self,
        files: &'a [(String, String)],
    ) -> BoxFuture<'a, Result<(), SandboxError>> {
        *self.staged.lock().unwrap_or_else(|e| e.into_inner()) = files.to_vec();
        Box::pin(async { Ok(()) })
    }

    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(command.to_string());
            self.results
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pop_front()
                .unwrap_or_else(|| Err(SandboxError::Runtime("no scripted result left".into())))
        })
    }
}

/// The input of [`SandboxTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SandboxInput {
    /// The shell command to run inside the Sandbox.
    pub command: String,
}

/// A Tool that runs a shell command in a Sandbox and returns its exit code and output.
#[derive(Clone)]
pub struct SandboxTool {
    sandbox: Arc<dyn Sandbox>,
}

impl SandboxTool {
    /// A Tool backed by `sandbox`.
    pub fn new(sandbox: Arc<dyn Sandbox>) -> Self {
        Self { sandbox }
    }
}

impl TypedTool for SandboxTool {
    type Input = SandboxInput;
    type Error = SandboxError;

    fn name(&self) -> String {
        "run_command".into()
    }

    fn description(&self) -> String {
        "Run a shell command in an isolated sandbox (no network, limited CPU, memory and time) \
         and return its exit code, stdout and stderr."
            .into()
    }

    async fn call(&self, input: SandboxInput) -> Result<String, SandboxError> {
        let out = self.sandbox.run(&input.command).await?;
        Ok(format!(
            "exit code: {}\nstdout:\n{}\nstderr:\n{}",
            out.exit_code, out.stdout, out.stderr
        ))
    }
}

/// Why an approved Skill script could not be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillScriptError {
    /// The script is unknown or its scripts are not approved; nothing ran.
    #[error(transparent)]
    Skill(#[from] SkillError),
    /// The Sandbox could not complete the script.
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
}

/// Runs the Role's approved Skill script inside `sandbox`. Scripts that are not
/// approved never reach the Sandbox, and there is no way to run one elsewhere.
pub async fn run_skill_script(
    library: &SkillLibrary,
    sandbox: &dyn Sandbox,
    role: &str,
    skill: &str,
    script: &str,
) -> Result<SandboxOutput, SkillScriptError> {
    let source = library.authorized_script(role, skill, script)?;
    Ok(sandbox.run(&source).await?)
}
