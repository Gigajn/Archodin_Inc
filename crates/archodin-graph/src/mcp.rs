//! A Model Context Protocol client: the tools of any MCP server become Tools.
//!
//! [`McpClient`] speaks MCP through the official `rmcp` SDK, to a server
//! started as a child process (stdio) or reached by URL (streamable HTTP).
//! Each tool the server lists becomes an [`AgentTool`] whose schema is the
//! server's own. Adding a server to a Role goes through [`McpLibrary`]:
//! the server stays unused until a person approves it through the
//! [`Approver`] interface, like a Skill script.
//!
//! The SDK's types stay inside this module: the seam is [`McpClient`] /
//! [`McpServer`] / [`McpLibrary`] and [`McpError`], and rmcp's
//! `RunningService` and `ClientHandler` never leak out (ADR 0009). The
//! lifecycle is `ClientLifecycleMode::Auto`: the client probes
//! `server/discover` for the 2026-07-28 protocol and falls back to the
//! legacy `initialize` handshake (2025-11-25) when the server does not
//! know discovery.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, ContentBlock,
        Implementation, PaginatedRequestParams, ProtocolVersion,
    },
    service::{ClientInitializeError, RunningService, ServiceError},
    transport::{
        streamable_http_client::{
            StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
        },
        TokioChildProcess,
    },
    ClientHandler, ClientLifecycleMode,
};
use serde_json::{json, Value};

use crate::http_guard::{shared_client, validate_endpoint, MAX_RESPONSE_BYTES};
use crate::locks::mutex;
use crate::{AgentTool, ApprovalDecision, ApprovalRequest, Approver};

/// The protocol version a legacy server is asked to speak when it does not
/// know `server/discover`.
const LEGACY_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V_2025_11_25;
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

/// The client side of the MCP conversation: Archodin asks the server for
/// nothing (no sampling, no roots, no elicitation) and only advertises its
/// own name and version.
#[derive(Clone, Default)]
struct ArchodinClientHandler;

impl ClientHandler for ArchodinClientHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("archodin", env!("CARGO_PKG_VERSION")),
        )
    }
}

/// The running rmcp client service. It owns the background task that pumps
/// the transport; dropping it closes the connection.
type RunningClient = RunningService<rmcp::RoleClient, ArchodinClientHandler>;

struct Inner {
    service: RunningClient,
}

/// A connection to one MCP server. Cloning gives another handle to it.
#[derive(Clone)]
pub struct McpClient {
    inner: Arc<Inner>,
}

impl McpClient {
    /// Connects to `server` and performs the MCP handshake.
    pub async fn connect(server: &McpServer) -> Result<Self, McpError> {
        let service = match &server.transport {
            Transport::Stdio { command, args } => {
                let mut cmd = tokio::process::Command::new(command);
                cmd.args(args);
                let (transport, _stderr) = TokioChildProcess::builder(cmd)
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| McpError::Transport(format!("cannot start `{command}`: {e}")))?;
                serve(transport).await?
            }
            Transport::Http { url } => {
                // The server URL is operator configuration (and an approved
                // server on top of that), so it goes through the operator
                // check — but it is still validated before anything connects,
                // and every request then uses the shared hardened client.
                validate_endpoint(url).map_err(|e| {
                    McpError::Transport(format!("refused MCP server URL {url}: {e}"))
                })?;
                let client = shared_client().map_err(|e| {
                    McpError::Transport(format!("could not build HTTP client: {e}"))
                })?;
                // `with_client` takes the plain `reqwest::Client` the crate
                // already hardens in `http_guard::shared_client()` — no
                // second client, no second TLS stack.
                // KR-01: the old hand-rolled client capped reads at 2 MiB;
                // rmcp's default is 16 MiB per SSE event, so pin it to the
                // same cap. This bounds one event, not a whole response —
                // acceptable here because an MCP server is operator-configured
                // and approved (see `McpLibrary`), not an arbitrary URL.
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.as_str());
                config.max_sse_event_size = MAX_RESPONSE_BYTES;
                let transport = StreamableHttpClientTransport::with_client(client, config);
                serve(transport).await?
            }
        };
        Ok(Self {
            inner: Arc::new(Inner { service }),
        })
    }

    /// The protocol version negotiated with the server, if the handshake ran:
    /// `2026-07-28` for a server that answered `server/discover`, `2025-11-25`
    /// for one that only knew the legacy `initialize` handshake.
    pub fn protocol_version(&self) -> Option<String> {
        self.inner
            .service
            .peer_info()
            .map(|info| info.protocol_version.as_str().to_string())
    }

    /// The tools the server lists, as Tools an Agent can call.
    pub async fn tools(&self) -> Result<Vec<AgentTool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = PaginatedRequestParams::default().with_cursor(cursor);
            let listed = self
                .call_with_timeout("tools/list", self.inner.service.list_tools(Some(params)))
                .await?;
            for tool in listed.tools {
                let name = tool.name.to_string();
                let description = tool.description.as_deref().unwrap_or_default();
                let schema = Value::Object((*tool.input_schema).clone());
                tools.push(self.tool(name, description, schema));
            }
            cursor = listed.next_cursor;
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
                let arguments = arguments.as_object().cloned().ok_or_else(|| {
                    "invalid arguments: tool arguments must be a JSON object".to_string()
                })?;
                let params = CallToolRequestParams::new(name).with_arguments(arguments);
                let result = client
                    .call_with_timeout("tools/call", client.inner.service.call_tool(params))
                    .await
                    .map_err(|e| e.to_string())?;
                let text = render_content(&result);
                if result.is_error == Some(true) {
                    Err(text)
                } else {
                    Ok(text)
                }
            })
        })
    }

    /// Runs one rmcp call, failing as a transport error if it takes longer
    /// than `REQUEST_TIMEOUT` instead of hanging a job forever.
    async fn call_with_timeout<F, T>(&self, method: &'static str, call: F) -> Result<T, McpError>
    where
        F: std::future::Future<Output = Result<T, ServiceError>>,
    {
        tokio::time::timeout(REQUEST_TIMEOUT, call)
            .await
            .map_err(|_| McpError::Transport(format!("`{method}` timed out")))?
            .map_err(map_service_error)
    }
}

/// Runs the `Auto` lifecycle over `transport`: probe `server/discover`
/// for 2026-07-28, falling back to the legacy `initialize` handshake
/// (2025-11-25) when the server does not know discovery. `allow_stateless`
/// stays at rmcp's default (`true`): a server that needs a session still
/// gets one, a stateless server just works.
async fn serve<T, E, A>(transport: T) -> Result<RunningClient, McpError>
where
    T: rmcp::transport::IntoTransport<rmcp::RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    rmcp::serve_client_with_lifecycle(
        ArchodinClientHandler,
        transport,
        ClientLifecycleMode::Auto {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            legacy_version: Some(LEGACY_PROTOCOL_VERSION),
        },
    )
    .await
    .map_err(map_init_error)
}

/// Maps an rmcp handshake failure onto the seam's error: a broken
/// connection is `Transport`, a JSON-RPC error from the server is `Server`,
/// anything else about the handshake is `Protocol`.
fn map_init_error(error: ClientInitializeError) -> McpError {
    match error {
        ClientInitializeError::TransportError { error, context } => {
            McpError::Transport(format!("{context}: {error}"))
        }
        ClientInitializeError::ConnectionClosed(message) => {
            McpError::Transport(format!("connection closed: {message}"))
        }
        ClientInitializeError::JsonRpcError(data) => McpError::Server {
            code: data.code.0 as i64,
            message: data.message.to_string(),
        },
        ClientInitializeError::LegacyFallbackFailed { discover, fallback } => {
            // Both phases failed: report the fallback (the last thing tried)
            // but keep the discover failure for context.
            let message = format!("legacy initialize failed after discover failed ({discover})");
            match map_init_error(*fallback) {
                McpError::Transport(detail) => McpError::Transport(format!("{message}: {detail}")),
                McpError::Protocol(detail) => McpError::Protocol(format!("{message}: {detail}")),
                McpError::Server { code, message: detail } => McpError::Server {
                    code,
                    message: format!("{message}: {detail}"),
                },
            }
        }
        ClientInitializeError::NoCompatibleProtocolVersion {
            client_supported,
            server_supported,
        } => McpError::Protocol(format!(
            "no compatible protocol version (client: {client_supported:?}, server: {server_supported:?})"
        )),
        ClientInitializeError::NoPreferredProtocolVersion => McpError::Protocol(
            "discover startup requires at least one preferred protocol version".into(),
        ),
        ClientInitializeError::Cancelled => McpError::Transport("connection cancelled".into()),
        other => McpError::Protocol(other.to_string()),
    }
}

/// Maps an rmcp call failure onto the seam's error: a JSON-RPC error from
/// the server is `Server` (with the server's code and message), a broken
/// connection or a timeout is `Transport`, anything else is `Protocol`.
fn map_service_error(error: ServiceError) -> McpError {
    match error {
        ServiceError::McpError(data) => McpError::Server {
            code: data.code.0 as i64,
            message: data.message.to_string(),
        },
        ServiceError::TransportSend(error) => McpError::Transport(error.to_string()),
        ServiceError::TransportClosed => {
            McpError::Transport("the server closed the connection".into())
        }
        ServiceError::Timeout { timeout } => {
            McpError::Transport(format!("request timed out after {timeout:?}"))
        }
        ServiceError::UnexpectedResponse => {
            McpError::Protocol("the server sent an unexpected response".into())
        }
        ServiceError::Cancelled { reason } => McpError::Transport(format!(
            "request cancelled{}",
            reason.map(|r| format!(": {r}")).unwrap_or_default()
        )),
        ServiceError::InputRequiredRoundsExceeded { max_rounds } => McpError::Protocol(format!(
            "the server kept asking for input beyond {max_rounds} rounds"
        )),
        other => McpError::Protocol(other.to_string()),
    }
}

/// Joins the text of a tool result's content; other content is kept as JSON.
fn render_content(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .map(|part| match part {
            ContentBlock::Text(text) => text.text.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        mutex(&self.inner).pending.push(PendingMcpServer {
            role: role.to_string(),
            server,
        });
    }

    /// The servers waiting for approval, oldest first.
    pub fn pending(&self) -> Vec<PendingMcpServer> {
        mutex(&self.inner).pending.clone()
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
            let mut inner = mutex(&self.inner);
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
        mutex(&self.inner)
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
            // `McpClient` has no `Debug`, so the whole `Result` cannot be
            // formatted — match and only format the `McpError` (which is
            // `Debug`) on failure.
            match block_on(McpClient::connect(&McpServer::http("x", url))) {
                Err(McpError::Transport(_)) => {}
                Err(e) => panic!("{url} must be refused with a Transport error, got {e:?}"),
                Ok(_) => panic!("{url} must be refused with a Transport error, but it connected"),
            }
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

    /// A fake MCP server over HTTP: one mock answers every JSON-RPC method by
    /// dispatching on the `method` field and echoing the request `id`, the way
    /// a real streamable HTTP server does. Notifications get an empty 202.
    /// The catch-all mock is created first: mockito answers with the last
    /// matching mock, so the notification mock created after it wins for its
    /// own method.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FakeServer {
        /// Answers `server/discover` with 2026-07-28: the modern path.
        Modern,
        /// Answers `server/discover` with "method not found": the legacy path.
        Legacy,
        /// Modern discovery, but `tools/list` answers with a JSON-RPC error.
        ToolsListError,
    }

    fn fake_http_server(kind: FakeServer) -> mockito::ServerGuard {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/mcp")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(move |request| {
                let body = request.body().map(|b| b.as_slice()).unwrap_or(&[]);
                let message: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
                let method = message.get("method").and_then(Value::as_str).unwrap_or("");
                let id = message.get("id").cloned().unwrap_or(Value::Null);
                let reply = match method {
                    "server/discover" => match kind {
                        FakeServer::Modern | FakeServer::ToolsListError => json!({
                            "jsonrpc": "2.0", "id": id, "result": {
                                "resultType": "complete",
                                "supportedVersions": ["2026-07-28"],
                                "capabilities": {},
                                "ttlMs": 0,
                                "cacheScope": "private",
                            }
                        }),
                        FakeServer::Legacy => json!({
                            "jsonrpc": "2.0", "id": id,
                            "error": {"code": -32601, "message": "Method not found"}
                        }),
                    },
                    "initialize" => json!({
                        "jsonrpc": "2.0", "id": id, "result": {
                            "protocolVersion": "2025-11-25",
                            "capabilities": {},
                            "serverInfo": {"name": "fake", "version": "1.0.0"},
                        }
                    }),
                    "tools/list" => match kind {
                        FakeServer::ToolsListError => json!({
                            "jsonrpc": "2.0", "id": id,
                            "error": {"code": -32602, "message": "bad cursor"}
                        }),
                        _ => json!({
                            "jsonrpc": "2.0", "id": id, "result": {
                                "tools": [{
                                    "name": "echo",
                                    "description": "echoes input",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {"text": {"type": "string"}},
                                    },
                                }]
                            }
                        }),
                    },
                    "tools/call" => json!({
                        "jsonrpc": "2.0", "id": id, "result": {
                            "content": [{"type": "text", "text": "hello http"}]
                        }
                    }),
                    _ => json!({
                        "jsonrpc": "2.0", "id": id,
                        "error": {"code": -32601, "message": "Method not found"}
                    }),
                };
                serde_json::to_vec(&reply).expect("a fake reply always serializes")
            })
            .create();
        server
            .mock("POST", "/mcp")
            .match_body(mockito::Matcher::Regex(
                r#""method"\s*:\s*"notifications/initialized""#.to_string(),
            ))
            .with_status(202)
            .create();
        server
    }

    /// A runtime that lives for the whole test. rmcp spawns background tasks
    /// (the transport worker, the serve pump) on it, so it must outlive every
    /// call: a fresh runtime per `block_on` would drop those tasks between
    /// `connect` and the first `tools()` call, closing the transport.
    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            // A current-thread runtime with no special settings always builds.
            .expect("a test runtime always builds")
    }

    /// `ClientLifecycleMode::Auto` must prefer `server/discover` when the
    /// server speaks 2026-07-28, and the tools must then work over HTTP.
    #[test]
    fn auto_handshake_prefers_discover_for_modern_http_server() {
        let rt = test_runtime();
        let server = fake_http_server(FakeServer::Modern);
        let url = format!("{}/mcp", server.url());
        let client = rt
            .block_on(McpClient::connect(&McpServer::http("fake", url)))
            .expect("the modern fake server must connect");
        assert_eq!(
            client.protocol_version().as_deref(),
            Some("2026-07-28"),
            "Auto must negotiate 2026-07-28 by discovery, not fall back to initialize"
        );
        let tools = rt.block_on(client.tools()).expect("tools/list must work");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "echo");
        assert_eq!(tools[0].description(), "echoes input");
        let answer = rt
            .block_on(tools[0].call(r#"{"text":"hi"}"#))
            .expect("tools/call must work");
        assert_eq!(answer, "hello http");
    }

    /// `ClientLifecycleMode::Auto` must fall back to the legacy `initialize`
    /// handshake (2025-11-25) when the server answers `server/discover`
    /// with "method not found".
    #[test]
    fn auto_handshake_falls_back_to_initialize_for_legacy_http_server() {
        let rt = test_runtime();
        let server = fake_http_server(FakeServer::Legacy);
        let url = format!("{}/mcp", server.url());
        let client = rt
            .block_on(McpClient::connect(&McpServer::http("fake", url)))
            .expect("the legacy fake server must connect");
        assert_eq!(
            client.protocol_version().as_deref(),
            Some("2025-11-25"),
            "Auto must fall back to the legacy initialize handshake at 2025-11-25"
        );
        let tools = rt.block_on(client.tools()).expect("tools/list must work");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "echo");
        let answer = rt
            .block_on(tools[0].call(r#"{"text":"hi"}"#))
            .expect("tools/call must work");
        assert_eq!(answer, "hello http");
    }

    /// A JSON-RPC error from the server maps to `McpError::Server` with the
    /// server's code and message — the rmcp `ServiceError::McpError` path.
    #[test]
    fn server_json_rpc_error_maps_to_mcp_server_error() {
        let rt = test_runtime();
        let server = fake_http_server(FakeServer::ToolsListError);
        let url = format!("{}/mcp", server.url());
        let client = rt
            .block_on(McpClient::connect(&McpServer::http("fake", url)))
            .expect("connect must succeed even when tools/list will fail");
        match rt.block_on(client.tools()) {
            Err(McpError::Server { code, message }) => {
                assert_eq!(code, -32602);
                assert_eq!(message, "bad cursor");
            }
            Err(e) => panic!("tools/list error must map to McpError::Server, got {e:?}"),
            Ok(tools) => panic!(
                "tools/list error must map to McpError::Server, but got {} tools",
                tools.len()
            ),
        }
    }

    /// A fake MCP server over stdio, speaking JSON-RPC one message per line:
    /// it answers `server/discover` with 2026-07-28 (the modern path), and
    /// `initialize` with 2025-11-25 for the legacy path.
    const FAKE_STDIO_SERVER: &str = r#"
import sys, json
for line in sys.stdin:
    try:
        req = json.loads(line)
    except Exception:
        continue
    method = req.get("method")
    rid = req.get("id")
    if method == "notifications/initialized":
        continue
    if method == "server/discover":
        result = {"resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {}, "ttlMs": 0, "cacheScope": "private"}
    elif method == "initialize":
        result = {"protocolVersion": "2025-11-25", "capabilities": {}, "serverInfo": {"name": "fake-stdio", "version": "1.0.0"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "echo", "description": "echoes input", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}]}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": "hello stdio"}]}
    else:
        if rid is not None:
            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "Method not found"}}) + "\n")
            sys.stdout.flush()
        continue
    if rid is not None:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "result": result}) + "\n")
        sys.stdout.flush()
"#;

    /// The stdio transport still works after the rmcp adoption, over the
    /// modern `server/discover` handshake.
    #[test]
    fn stdio_server_still_works_over_the_modern_handshake() {
        let rt = test_runtime();
        let client = rt
            .block_on(McpClient::connect(&McpServer::stdio(
                "fake-stdio",
                "python3",
                ["-c", FAKE_STDIO_SERVER],
            )))
            .expect("the fake stdio server must connect");
        assert_eq!(
            client.protocol_version().as_deref(),
            Some("2026-07-28"),
            "Auto must negotiate 2026-07-28 by discovery over stdio too"
        );
        let tools = rt
            .block_on(client.tools())
            .expect("tools/list must work over stdio");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "echo");
        let answer = rt
            .block_on(tools[0].call(r#"{"text":"hi"}"#))
            .expect("tools/call must work over stdio");
        assert_eq!(answer, "hello stdio");
    }
}
