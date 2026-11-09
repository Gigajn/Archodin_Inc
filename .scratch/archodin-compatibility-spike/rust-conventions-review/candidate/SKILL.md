---
name: rust-conventions
description: Use when writing or reviewing Rust in this repo: generic Archodin contracts, Tokio lifecycle, typed errors and offline seams; scoped Basileia and legacy parity guidance when maintaining application code.
---

# Rust conventions

## Scope

Read the project's glossary and relevant ADRs before choosing a contract. Archodin owns generic library mechanisms; Basileia owns company Agents, prompts, workflows and organizational persistence. Keep application policy out of generic library APIs.

When maintaining Basileia application persistence or the legacy AAHC Python-parity router, read [legacy-application.md](legacy-application.md). Apply its rules only to that existing application scope; generic Provider/Tool work does not inherit them.

Crate and type names below illustrate techniques, not permission to add dependencies. Inspect the workspace first; justify additions against the ticket, license and compatibility evidence. The integration dependency remains undecided until the compatibility spike compares candidates; langchain-rust is a candidate, not a requirement.

## Errors and secrets

- Use a typed error enum at each public boundary with actionable caller data, independent of vendor SDK types. `thiserror` is an option when available; avoid `Box<dyn Error>` in public library contracts.
- At a binary edge, add operational context using the existing error pattern (`anyhow` where available). Map HTTP failures deliberately instead of exposing panics or credential-bearing source messages.
- Propagate recoverable failures with `?`. Reserve `unwrap`/`expect` for tests or proven invariants whose message explains the proof.
- Obtain production keys through the application's configured secret boundary (currently environment variables), not a library-specific environment lookup. Log variable names, never values. Redact credential-bearing `Debug`, errors and diagnostics; keep keys out of URLs and fixtures.

## Async / Tokio

- Reuse the caller's Tokio runtime; avoid nested runtimes and mixed executors.
- Keep synchronous I/O and CPU work off async workers. Use bounded blocking work (`spawn_blocking` where appropriate); consider a CPU pool only if justified. Started blocking work cannot be forcibly cancelled by dropping its future.
- Bound ingress queues and concurrency so backpressure limits resource growth.
- Bound each network call and child process with a deadline. A timeout drops a future, not remote computation: explicitly terminate/reap owned child processes and stop/join owned tasks as their APIs require.
- Propagate per-operation cancellation to child work (`CancellationToken` when available); long loops cooperate. Track and join spawned tasks (`JoinSet` when appropriate), including timeout, cancellation and shutdown paths. A dropped task handle is not cleanup.
- Prefer short synchronous lock scopes with no guard across `.await`; use an async mutex only when awaiting while locked is necessary.

## Ownership and testing

- Borrow in internals (`&str`, `&[u8]`), own at lifetime boundaries. Clone cheap shared handles rather than large data.
- Exercise public contracts with injected scripted transports/adapters and harmless typed Tools. Default tests need no credentials, network or inference spend; isolate and skip live-service tests by default.
- Validate structured Tool input before execution. Assert outputs and typed error variants, not private collaborators or error prose. Test names describe behavior.
- Verify deadlines, cancellation and owned-work cleanup with controlled clocks and synchronization rather than fragile sleeps. A generic fake does not prove real SDK/protocol mapping; use adapter fixtures for that evidence.
- Follow the separate TDD skill for agreed seams and one red→green behavior at a time; keep clean-code and token-efficiency as separate references rather than duplicating them here.

## Gate

Run the repository's `scripts/check.sh`; formatting, Clippy warnings-as-errors and tests must pass. Record actual results and limitations rather than treating a proposed test as executed evidence.
