//! A Model Context Protocol client: the tools of any MCP server become Tools.
//!
//! [`McpClient`] speaks JSON-RPC 2.0 to a server started as a child process (stdio,
//! one message per line) or reached by URL (streamable HTTP, replies as JSON or as
//! an event stream). Each tool the server lists becomes an [`AgentTool`] whose
//! schema is the server's own. Adding a server to a Role goes through
//! [`McpLibrary`]: the server stays unused until a person approves it through the
//! [`Approver`] interface, like a Skill script.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::http_guard::{read_capped, shared_client, text_capped, validate_endpoint};
use crate::{AgentTool, ApprovalDecision, ApprovalRequest, Approver};

const PROTOCOL_VERSION: &str = "2025-03-26";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Where and how to reach an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    name: String,
    transport: Transport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Transport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
}

impl McpServer {
    /// A server started as a child process that talks over stdin and stdout.
    pub fn stdio<A: Into<String>>(
        name: impl Into<String>,
        command: impl Into<String>,
        args: impl IntoIterator<Item = A>,
    ) -> Self {
        Self {
            name: name.into(),
            transport: Transport::Stdio {
                command: command.into(),
                args: args.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// A server reached over streamable HTTP at `url`.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: Transport::Http { url: url.into() },
        }
    }

    /// The name the server is known by.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn describe(&self) -> String {
        match &self.transport {
            Transport::Stdio { command, args } => format!("stdio `{command} {}`", args.join(" ")),
            Transport::Http { url } => format!("http {url}"),
        }
    }
}

/// Why talking to an MCP server failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum McpError {
    /// The server could not be reached, or the connection broke.
    #[error("mcp transport error: {0}")]
    Transport(String),
    /// The server sent something that is not valid MCP.
    #[error("mcp protocol error: {0}")]
    Protocol(String),
    /// The server answered a request with a JSON-RPC error.
    #[error("mcp server error {code}: {message}")]
    Server { code: i64, message: String },
}

struct StdioConn {
    _child: Child,
    stdin: ChildStdin,
    stdout: tokio::io::Lines<BufReader<ChildStdout>>,
}

enum Conn {
    Stdio(Box<tokio::sync::Mutex<StdioConn>>),
    Http {
        client: reqwest::Client,
        url: String,
        session: Mutex<Option<String>>,
    },
}

struct Inner {
    conn: Conn,
    next_id: AtomicU64,
}

/// A connection to one MCP server. Cloning gives another handle to it.
#[derive(Clone)]
pub struct McpClient {
    inner: Arc<Inner>,
}

impl McpClient {
    /// Connects to `server` and performs the MCP handshake.
    pub async fn connect(server: &McpServer) -> Result<Self, McpError> {
        let conn = match &server.transport {
            Transport::Stdio { command, args } => {
                let mut child = tokio::process::Command::new(command)
                    .args(args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| McpError::Transport(format!("cannot start `{command}`: {e}")))?;
                let stdin = child.stdin.take().expect("stdin is piped");
                let stdout = child.stdout.take().expect("stdout is piped");
                Conn::Stdio(Box::new(tokio::sync::Mutex::new(StdioConn {
                    _child: child,
                    stdin,
                    stdout: BufReader::new(stdout).lines(),
                })))
            }
            Transport::Http { url } => {
                // The server URL is operator configuration (and an approved
                // server on top of that), so it goes through the operator
                // check — but it is still validated before anything connects,
                // and every request then uses the shared hardened client.
                validate_endpoint(url).map_err(|e| {
                    McpError::Transport(format!("refused MCP server URL {url}: {e}"))
                })?;
                Conn::Http {
                    client: shared_client().map_err(|e| {
                        McpError::Transport(format!("could not build HTTP client: {e}"))
                    })?,
                    url: url.clone(),
                    session: Mutex::new(None),
                }
            }
        };
        let client = Self {
            inner: Arc::new(Inner {
                conn,
                next_id: AtomicU64::new(1),
            }),
        };
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "archodin", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await?;
        client.notify("notifications/initialized").await?;
        Ok(client)
    }

    /// The tools the server lists, as Tools an Agent can call.
    pub async fn tools(&self) -> Result<Vec<AgentTool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let result = self.request("tools/list", params).await?;
            let listed = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpError::Protocol("tools/list has no `tools` array".into()))?;
            for tool in listed {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::Protocol("a tool has no name".into()))?
                    .to_string();
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let schema = tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"}));
                tools.push(self.tool(name, description, schema));
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                return Ok(tools);
            }
        }
    }

    fn tool(&self, name: String, description: &str, schema: Value) -> AgentTool {
        let client = self.clone();
        let tool_name = name.clone();
        AgentTool::dynamic(name, description, schema, move |arguments| {
            let client = client.clone();
            let name = tool_name.clone();
            Box::pin(async move {
                let arguments: Value = if arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&arguments)
                        .map_err(|e| format!("invalid arguments: {e}"))?
                };
                let result = client
                    .request("tools/call", json!({"name": name, "arguments": arguments}))
                    .await
                    .map_err(|e| e.to_string())?;
                let text = render_content(&result);
                if result.get("isError").and_then(Value::as_bool) == Some(true) {
                    Err(text)
                } else {
                    Ok(text)
                }
            })
        })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let reply = tokio::time::timeout(REQUEST_TIMEOUT, self.exchange(id, message))
            .await
            .map_err(|_| McpError::Transport(format!("`{method}` timed out")))??;
        if let Some(error) = reply.get("error") {
            return Err(McpError::Server {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            });
        }
        reply
            .get("result")
            .cloned()
            .ok_or_else(|| McpError::Protocol(format!("`{method}` reply has no result")))
    }

    async fn notify(&self, method: &str) -> Result<(), McpError> {
        let message = json!({"jsonrpc": "2.0", "method": method});
        match &self.inner.conn {
            Conn::Stdio(conn) => write_line(&mut *conn.lock().await, &message).await,
            Conn::Http { .. } => self.http_post(&message).await.map(|_| ()),
        }
    }

    /// Sends a request and returns the reply with the same id.
    async fn exchange(&self, id: u64, message: Value) -> Result<Value, McpError> {
        match &self.inner.conn {
            Conn::Stdio(conn) => {
                let mut conn = conn.lock().await;
                write_line(&mut conn, &message).await?;
                loop {
                    let line = conn
                        .stdout
                        .next_line()
                        .await
                        .map_err(|e| McpError::Transport(e.to_string()))?
                        .ok_or_else(|| McpError::Transport("the server closed stdout".into()))?;
                    if let Ok(reply) = serde_json::from_str::<Value>(&line) {
                        if reply.get("id") == Some(&json!(id)) {
                            return Ok(reply);
                        }
                    }
                }
            }
            Conn::Http { .. } => {
                let body = self.http_post(&message).await?;
                find_reply(&body, id)
            }
        }
    }

    async fn http_post(&self, message: &Value) -> Result<String, McpError> {
        let Conn::Http {
            client,
            url,
            session,
        } = &self.inner.conn
        else {
            unreachable!("only called for HTTP connections");
        };
        let mut request = client
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .json(message);
        if let Some(id) = lock(session).clone() {
            request = request.header("Mcp-Session-Id", id);
        }
        let response = request
            .send()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        if !response.status().is_success() {
            return Err(McpError::Transport(format!(
                "server answered {}",
                response.status()
            )));
        }
        if let Some(id) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *lock(session) = Some(id.to_string());
        }
        // The reply is read capped at 2 MiB instead of unbounded (KR-01).
        let body = read_capped(response)
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;
        Ok(text_capped(body))
    }
}

async fn write_line(conn: &mut StdioConn, message: &Value) -> Result<(), McpError> {
    let mut line = message.to_string();
    line.push('\n');
    conn.stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| McpError::Transport(e.to_string()))?;
    conn.stdin
        .flush()
        .await
        .map_err(|e| McpError::Transport(e.to_string()))
}

/// Finds the reply with `id` in an HTTP body: a single JSON message, a batch, or
/// an event stream whose `data:` lines carry messages.
fn find_reply(body: &str, id: u64) -> Result<Value, McpError> {
    let mut candidates: Vec<Value> = Vec::new();
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Array(items)) => candidates.extend(items),
        Ok(value) => candidates.push(value),
        Err(_) => candidates.extend(
            body.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .filter_map(|d| serde_json::from_str(d.trim()).ok()),
        ),
    }
    candidates
        .into_iter()
        .find(|m| m.get("id") == Some(&json!(id)))
        .ok_or_else(|| McpError::Protocol(format!("no reply to request {id}")))
}

/// Joins the text of a tool result's content; other content is kept as JSON.
fn render_content(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .map(|p| match p.get("text").and_then(Value::as_str) {
                    Some(text) if p.get("type").and_then(Value::as_str) == Some("text") => {
                        text.to_string()
                    }
                    _ => p.to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// An MCP server waiting for a person to approve it for a Role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMcpServer {
    /// The Role that would get the server's tools.
    pub role: String,
    /// The proposed server.
    pub server: McpServer,
}

#[derive(Default)]
struct Registry {
    pending: Vec<PendingMcpServer>,
    approved: HashMap<String, Vec<McpServer>>,
}

/// Holds the MCP servers proposed for Roles until a person approves them, and
/// gives each Role the tools of its approved servers. Cloning gives another
/// handle to the same library.
#[derive(Clone, Default)]
pub struct McpLibrary {
    inner: Arc<Mutex<Registry>>,
}

impl McpLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds `server` for `role` until a person approves it.
    pub fn propose(&self, role: &str, server: McpServer) {
        lock(&self.inner).pending.push(PendingMcpServer {
            role: role.to_string(),
            server,
        });
    }

    /// The servers waiting for approval, oldest first.
    pub fn pending(&self) -> Vec<PendingMcpServer> {
        lock(&self.inner).pending.clone()
    }

    /// Asks `approver` about every pending server. Approved ones are given to
    /// their Role, rejected ones are dropped, undecided ones stay pending.
    pub async fn review_pending(&self, approver: &dyn Approver) {
        for item in self.pending() {
            let request = ApprovalRequest {
                job: format!("mcp/{}", item.role),
                node: item.server.name.clone(),
                prompt: format!(
                    "Approve MCP server `{}` ({}) for role `{}`",
                    item.server.name,
                    item.server.describe(),
                    item.role
                ),
            };
            let decision = approver.decide(&request).await;
            if decision == ApprovalDecision::Pending {
                continue;
            }
            let mut inner = lock(&self.inner);
            if let Some(i) = inner.pending.iter().position(|p| *p == item) {
                inner.pending.remove(i);
            }
            if decision == ApprovalDecision::Approved {
                inner
                    .approved
                    .entry(item.role)
                    .or_default()
                    .push(item.server);
            }
        }
    }

    /// The approved servers of a Role.
    pub fn servers_for_role(&self, role: &str) -> Vec<McpServer> {
        lock(&self.inner)
            .approved
            .get(role)
            .cloned()
            .unwrap_or_default()
    }

    /// Connects to every approved server of `role` and returns their tools.
    pub async fn tools_for_role(&self, role: &str) -> Result<Vec<AgentTool>, McpError> {
        let mut tools = Vec::new();
        for server in self.servers_for_role(role) {
            tools.extend(McpClient::connect(&server).await?.tools().await?);
        }
        Ok(tools)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            // A current-thread runtime with no special settings always builds.
            .expect("a test runtime always builds")
            .block_on(future)
    }

    /// A malformed server URL fails in `connect` before anything connects —
    /// so these tests need no network.
    #[test]
    fn http_server_with_a_bad_url_is_refused_before_connecting() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/mcp",
            "https://user:pass@example.com/mcp",
            "https://",
            "not a url",
        ] {
            let err = block_on(McpClient::connect(&McpServer::http("x", url)));
            assert!(
                matches!(err, Err(McpError::Transport(_))),
                "{url} must be refused with a Transport error, got {err:?}"
            );
        }
    }

    /// An operator-configured loopback server is allowed through validation:
    /// nothing listens on the port, so connecting fails fast instead of
    /// being refused. (The port is taken from a listener that is closed
    /// first, so nothing can be listening on it — but if another test's
    /// server happens to reuse the port in that short window, the failure
    /// is some other error, not a refusal; the assertion only pins down
    /// "attempted, not refused".)
    #[test]
    fn http_server_on_loopback_is_attempted_not_blocked() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("binding a loopback listener always works")
            .local_addr()
            .expect("a bound listener always has an address")
            .port();
        let err = block_on(McpClient::connect(&McpServer::http(
            "x",
            format!("http://127.0.0.1:{port}/mcp"),
        )));
        assert!(
            err.is_err(),
            "connecting to a closed loopback port must fail"
        );
        if let Err(McpError::Transport(message)) = &err {
            assert!(
                !message.contains("refused"),
                "loopback must be attempted, not refused, got {message:?}"
            );
        }
    }
}
