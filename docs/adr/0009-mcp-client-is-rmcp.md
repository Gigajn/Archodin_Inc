# The MCP client is rmcp, not hand-rolled JSON-RPC

The MCP client in `mcp.rs` is replaced by the official Rust SDK `rmcp` (v3.9.0, Apache-2.0), behind the existing `McpClient` / `McpServer` / `McpLibrary` seam. The hand-rolled JSON-RPC 2.0 client goes away, including its `PROTOCOL_VERSION = "2025-03-26"`, which is already two spec generations behind. rmcp implements the stable 2026-07-28 spec (`server/discover`, stateless) with a 2025-11-25 fallback, and its Streamable HTTP transport runs on reqwest 0.13.2 — the same reqwest Archodin uses.

Two choices are made explicitly, because rmcp's defaults do not match Archodin's: (1) the lifecycle is `ClientLifecycleMode::Auto` (probe `server/discover` with 2026-07-28 preferred, fall back to the legacy `Initialize` handshake with 2025-11-25), never the default `Initialize`-only mode — the servers Archodin actually talks to may still be legacy, and `Auto` is what covers both; (2) the TLS provider is native-tls (rmcp's `reqwest-native-tls` feature), because Archodin already uses native-tls and a second TLS stack (rmcp's default rustls) is not acceptable. Features are minimal: `client` + `transport-streamable-http-client-reqwest`, plus `auth` only if OAuth is actually needed, and `elicitation` only if it is actually needed. `allow_stateless` stays at its default (`true`) unless a server Archodin uses requires sessions.

## Why this crate, and the hard questions

- **Why replace ~549 lines of working code?** The hand-rolled client is protocol work Archodin maintains by itself: handshake, version negotiation, SSE parsing, session handling, error mapping. rmcp is the official SDK, conformance-tested, and carries the features Archodin would otherwise hand-roll badly (OAuth, tasks, elicitation, resources, prompts). The leverage is in not owning protocol conformance.
- **What if a server Archodin uses only speaks the old spec?** That is what `Auto` is for: it probes `server/discover` and falls back to legacy within 10 seconds. If a server speaks neither, that server is out of spec and the failure is a protocol error, not a silent downgrade.
- **What about the stdio transport?** rmcp has `transport-child-process` (stdio) alongside Streamable HTTP, so the `McpServer::stdio` constructor keeps working behind the same seam.
- **Sampling / roots / logging are deprecated (SEP-2577)?** That is the spec's direction, not a gap in rmcp. Archodin only needs tools; the deprecation does not affect the adoption.
- **What if rmcp's `RunningService` API does not fit `McpLibrary`'s approval flow?** The approval flow (a server stays unused until a person approves it) lives in `McpLibrary`, above the client. rmcp sits below that seam; if the fit is bad, the seam is where the mismatch is contained, not spread across callers.

## Considered Options

- **Keep the hand-rolled client and bump `PROTOCOL_VERSION`**: the smallest change, but Archodin keeps owning handshake, negotiation, SSE and session handling, and every future spec change is Archodin's work.
- **Use `rust-mcp-sdk` instead**: it has a fuller transport list (including legacy SSE) and a richer OAuth client, but a far smaller community (199 stars) and the same spec coverage; the official SDK wins on maintenance.
- **Use `pmcp`**: still on spec 2025-11-25 with ambiguous activity; rejected.
