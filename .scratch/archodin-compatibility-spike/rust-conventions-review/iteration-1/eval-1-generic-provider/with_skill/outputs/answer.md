# Proposed Provider contract

## Boundary and API

Archodin owns generic mechanisms; Basileia supplies company agents, prompts, workflows, secrets and retry policy. No integration dependency is selected; langchain-rust is only a candidate.

Define an object-safe `Provider: Send + Sync` with:

- `capabilities(&self) -> ProviderCapabilities`.
- `complete(&self, request: CompletionRequest, context: OperationContext) -> Pin<Box<dyn Future<Output = Result<CompletionResponse, ProviderError>> + Send + '_>>`.

This supports generic consumers and `Arc<dyn Provider>` using standard-library types, without requiring async-trait. Borrow the provider; own request/context data across the asynchronous boundary.

`CompletionRequest` contains a model identifier, ordered role/content messages, optional output-token limit, and optional tool definitions. `CompletionResponse` contains content blocks, tool-call identifiers and arguments, finish reason, and optional usage counts. Distinguish unavailable usage from zero. Use library-owned structured values, not SDK types. Tool calls are data, not permission to execute; callers validate arguments and enforce approvals.

Capabilities describe supported content, tools and limits. Reject unsupported requests instead of silently dropping fields. Leave streaming to a separate extension rather than promising it through this one-shot contract.

## Execution and errors

`OperationContext` carries an absolute deadline and a cloneable cooperative cancellation signal. Implementation and runtime integration remain undecided; neither should expose vendor types. Adapters reuse the caller's runtime, bound network operations and concurrency, and propagate cancellation. Owned tasks must be stopped and joined; owned processes terminated and reaped. Dropping a future does not prove remote computation stopped. Avoid blocking async workers or holding synchronous locks across awaits.

Expose a non-exhaustive typed `ProviderError`: `InvalidRequest`, `UnsupportedCapability`, `Authentication`, `PermissionDenied`, `RateLimited { retry_after }`, `DeadlineExceeded`, `Cancelled`, `Unavailable`, `Transport`, and `InvalidResponse`. Include safe field/capability identifiers and optional status codes, never raw credential-bearing bodies or vendor errors. Do not automatically retry potentially billable requests. Accept credentials through application-injected adapter configuration; redact secrets in diagnostics and `Debug`.

## Testing and unknowns

Develop one public behavior red→green at a time using scripted adapters: response preservation, unsupported requests, error variants, missing usage, and secret redaction. Use controlled clocks and synchronization for deadlines, cancellation, bounded concurrency and owned-work cleanup. Validate tool input before execution using harmless typed tools. Default tests need no network, credentials or inference spend.

Adapter protocol fixtures must separately establish real mapping; generic fakes cannot. Before selecting dependencies, verify MIT distribution compatibility, notices, MSRV and runtime compatibility. These remain unverified. This is a design only; no tests or repository gate were run.
