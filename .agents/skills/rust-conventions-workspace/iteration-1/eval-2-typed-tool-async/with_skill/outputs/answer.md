# Typed Tool and deterministic async fake

## Contract

Keep this an Archodin library mechanism: no Basileia workflows, company agents, or persistence policy.

Define a statically dispatched `Tool` with associated `Input` and `Output` types and an asynchronous `execute(&self, input, context)` returning `Result<Output, ToolError>`. Require the returned future to be `Send` if the caller will spawn it; do not assume trait-object compatibility. The operation context carries an absolute deadline and a caller-controlled cancellation signal, without prescribing a new dependency.

Use a harmless `AddTool`: raw input contains two integers; validation constructs `AddInput` with documented bounds. Execution uses checked addition and returns `AddOutput { sum }`. Invalid input must fail before invoking the execution adapter or consuming a scripted step.

Expose typed errors such as `InvalidInput { field, reason }`, `Overflow`, `DeadlineExceeded`, `Cancelled`, and `ExecutionFailed { code }`. Keep vendor errors and credential-bearing messages out of this boundary. Fake-only configuration errors can use a distinct typed test error mapped into `ExecutionFailed`, never a panic in the public contract.

## Scripted fake

Inject a `ScriptedTool` implementing the same contract, with a bounded FIFO of steps: `Return(output)`, `Fail(error)`, and `Hang`. Pop a step under a short synchronous lock, then release the guard before awaiting. Exhaustion returns a typed failure. Use sequential invocations for deterministic script ordering; concurrency ordering is not promised.

`Hang` acknowledges entry through a synchronization signal, then awaits a never-ready future. A drop guard signals that the in-flight future was released. This fake spawns no tasks and performs no I/O, so dropping that future fully cleans up its owned work.

## Lifecycle

Reuse the caller’s Tokio runtime. A runner validates first, then races execution against cancellation and the remaining absolute deadline; specify cancellation precedence when multiple branches are ready. Dropping execution does not stop remote computation. Any future adapter that owns tasks or processes must propagate cancellation and stop/join tasks or terminate/reap processes before reporting completion. Do not introduce detached work or blocking work into this fake. Bound accepted concurrency rather than permitting an unbounded waiting queue.

## Proposed verification

Implement one red→green behavior at a time: valid result; invalid input without execution; typed scripted failure; script exhaustion; hanging-call deadline; caller cancellation. For hanging cases, await the entry signal, advance a controlled clock where needed, and assert the exact error variant plus the cleanup signal. Avoid wall-clock sleeps. Adapter fixtures would separately establish real protocol/error mappings.

This is a plan only. Workspace APIs, dependencies, and clock/cancellation facilities were not inspected; no tests or repository gate were run, and no dependency is selected.
