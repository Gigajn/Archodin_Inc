//! The MCP client: tools of MCP servers become Tools, and a Role only gets a
//! server a person approved.

use archodin::graph::{
    ApprovalDecision, McpClient, McpError, McpLibrary, McpServer, ScriptedApprover,
};
use mockito::{Matcher, Server};
use serde_json::{json, Value};

/// A fake stdio MCP server: one JSON-RPC message per line.
const FAKE_STDIO: &str = r#"
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *'"server/discover"'*)
      # A legacy server: it does not know discovery, so the client falls
      # back to the `initialize` handshake.
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id";;
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id";;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","method":"notifications/message","params":{}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echoes text","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}}\n' "$id";;
    *'"tools/call"'*'"fail"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"isError":true,"content":[{"type":"text","text":"boom"}]}}\n' "$id";;
    *'"tools/call"'*)
      text=$(printf '%s' "$line" | sed -n 's/.*"text":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"echo: %s"}]}}\n' "$id" "$text";;
  esac
done
"#;

fn stdio_server() -> McpServer {
    McpServer::stdio("fake", "sh", ["-c", FAKE_STDIO])
}

#[tokio::test]
async fn stdio_server_tools_are_typed_tools_an_agent_can_call() {
    let client = McpClient::connect(&stdio_server()).await.unwrap();
    let tools = client.tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name(), "echo");
    assert_eq!(tools[0].description(), "Echoes text");
    assert_eq!(tools[0].parameters()["required"], json!(["text"]));
    assert_eq!(tools[0].call(r#"{"text":"hi"}"#).await.unwrap(), "echo: hi");
    assert_eq!(
        tools[0].call(r#"{"text":"fail"}"#).await,
        Err("boom".into())
    );
}

#[tokio::test]
async fn missing_stdio_command_is_an_error() {
    let server = McpServer::stdio("x", "/no/such/command", Vec::<String>::new());
    assert!(matches!(
        McpClient::connect(&server).await,
        Err(McpError::Transport(_))
    ));
}

fn rpc_body(id: u64, result: Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
}

#[tokio::test]
async fn http_server_tools_work_over_json_and_event_streams() {
    let mut server = Server::new_async().await;
    // A legacy server: `server/discover` is unknown, so the client falls
    // back to the `initialize` handshake. rmcp numbers requests from 0,
    // so discovery is id 0 and the old fixed ids below still line up.
    let discover = server
        .mock("POST", "/mcp")
        .match_body(Matcher::PartialJson(json!({"method":"server/discover"})))
        .with_header("content-type", "application/json")
        .with_body(
            json!({"jsonrpc":"2.0","id":0,"error":{"code":-32601,"message":"Method not found"}})
                .to_string(),
        )
        .create_async()
        .await;
    let init = server
        .mock("POST", "/mcp")
        .match_body(Matcher::PartialJson(json!({"method":"initialize"})))
        .with_header("content-type", "application/json")
        .with_header("mcp-session-id", "s1")
        .with_body(rpc_body(
            1,
            json!({"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"f","version":"1"}}),
        ))
        .create_async()
        .await;
    let initialized = server
        .mock("POST", "/mcp")
        .match_header("mcp-session-id", "s1")
        .match_body(Matcher::PartialJson(
            json!({"method":"notifications/initialized"}),
        ))
        .with_status(202)
        .create_async()
        .await;
    let list = server
        .mock("POST", "/mcp")
        .match_header("mcp-session-id", "s1")
        .match_body(Matcher::PartialJson(json!({"method":"tools/list"})))
        .with_header("content-type", "application/json")
        .with_body(rpc_body(
            2,
            json!({"tools":[{"name":"add","inputSchema":{"type":"object"}}]}),
        ))
        .create_async()
        .await;
    let call = server
        .mock("POST", "/mcp")
        .match_header("mcp-session-id", "s1")
        .match_body(Matcher::PartialJson(
            json!({"method":"tools/call","params":{"name":"add","arguments":{"a":1}}}),
        ))
        .with_header("content-type", "text/event-stream")
        .with_body(format!(
            "event: message\ndata: {}\n\nevent: message\ndata: {}\n\n",
            json!({"jsonrpc":"2.0","method":"notifications/progress","params":{}}),
            rpc_body(3, json!({"content":[{"type":"text","text":"sum"}]}))
        ))
        .create_async()
        .await;

    let client = McpClient::connect(&McpServer::http("web", format!("{}/mcp", server.url())))
        .await
        .unwrap();
    let tools = client.tools().await.unwrap();
    assert_eq!(tools[0].name(), "add");
    assert_eq!(tools[0].call(r#"{"a":1}"#).await.unwrap(), "sum");
    for m in [discover, init, initialized, list, call] {
        m.assert_async().await;
    }
}

#[tokio::test]
async fn http_error_status_is_an_error() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/mcp")
        .with_status(500)
        .create_async()
        .await;
    let result = McpClient::connect(&McpServer::http("web", format!("{}/mcp", server.url()))).await;
    assert!(matches!(result, Err(McpError::Transport(_))));
}

#[tokio::test]
async fn a_role_gets_an_mcp_server_only_after_approval() {
    let lib = McpLibrary::new();
    lib.propose("Engineer", stdio_server());
    assert_eq!(lib.pending().len(), 1);
    assert!(lib.tools_for_role("Engineer").await.unwrap().is_empty());

    let pending = ScriptedApprover::new([ApprovalDecision::Pending]);
    lib.review_pending(&pending).await;
    assert_eq!(lib.pending().len(), 1);
    assert!(lib.tools_for_role("Engineer").await.unwrap().is_empty());

    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    lib.review_pending(&approver).await;
    assert!(approver.requests()[0].prompt.contains("fake"));
    assert!(approver.requests()[0].prompt.contains("Engineer"));
    assert!(lib.pending().is_empty());
    assert_eq!(
        lib.tools_for_role("Engineer").await.unwrap()[0].name(),
        "echo"
    );
    assert!(lib.tools_for_role("Designer").await.unwrap().is_empty());
}

#[tokio::test]
async fn a_rejected_mcp_server_is_dropped() {
    let lib = McpLibrary::new();
    lib.propose("Engineer", stdio_server());
    lib.review_pending(&ScriptedApprover::new([ApprovalDecision::Rejected]))
        .await;
    assert!(lib.pending().is_empty());
    assert!(lib.tools_for_role("Engineer").await.unwrap().is_empty());
}
