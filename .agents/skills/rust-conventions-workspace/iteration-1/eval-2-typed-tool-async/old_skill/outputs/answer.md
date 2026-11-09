# Typed Tool with a scripted asynchronous fake

## Contract

Define a `Tool` trait with associated `Input` and `Output` types and an asynchronous `call(&self, input)` returning `Result<Output, ToolError>`. Prefer generic injection (`Runner<T: Tool>`) rather than requiring dynamic dispatch. Require a `Send` future if calls must run in Tokio tasks; the concrete trait syntax depends on the repository's Rust version.

Use one typed `thiserror` error enum at the tool boundary: `InvalidInput`, `Transport`, `TimedOut { limit: Duration }`, `Cancelled`, and `UnexpectedCall`. Variants carry actionable, non-secret data. Preserve existing domain error codes when applicable; their relevance to this Tool is unspecified. Avoid boxed generic errors, string matching, and recoverable panics.

## Execution boundary

The runner owns the time budget and a per-work-unit `CancellationToken`. Race cancellation against `tokio::time::timeout(limit, tool.call(input))`, mapping outcomes to typed variants. Specify cancellation precedence when multiple outcomes are ready, using a cancellation-first biased selection if required.

Keep the call future inline: expiration or cancellation drops it, rather than leaving a spawned operation running. This requires cancellation-safe implementations. Dropping a future does not undo remote effects or stop already-running blocking work; mutating tools need an explicit idempotency/reconciliation policy. Any necessary child tasks inherit the token and are joined during shutdown. Production network calls and child processes also receive timeouts.

## Scripted fake

Inject `ScriptedTool` with a FIFO `VecDeque<Step>`, where steps contain an expected typed input and an action: `Return(output)`, `Fail(ToolError)`, or `Hang`.

Protect the queue and recorded inputs with `std::sync::Mutex`; record the call and remove its step in a short scope, releasing the guard before awaiting. Missing steps or input mismatches produce `UnexpectedCall`, not a panic. `Hang` awaits `std::future::pending()` without blocking a thread or spawning a task. A drop guard can record cancellation cleanup without retaining a mutex guard across await. Expose recorded calls and remaining-step count for assertions.

## Planned offline tests

Develop one red/green behavior at a time:

- A successful step returns its typed output and consumes exactly one step.
- A failure preserves the typed variant.
- A mismatched or exhausted script reports `UnexpectedCall`.
- A hanging call becomes `TimedOut`; its future is dropped.
- Explicit cancellation becomes `Cancelled`; its future is dropped.

Use Tokio's paused clock and explicit time advancement if existing test features permit; otherwise use bounded real-time tests. Synchronize on fake entry before advancing time or cancelling. Assert variants, call history, cleanup, and script consumption—not error text.

This is a design only: no dependencies added, implementation inspected, or tests executed.
