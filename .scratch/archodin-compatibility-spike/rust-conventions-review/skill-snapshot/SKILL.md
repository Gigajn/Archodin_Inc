---
name: rust-conventions
description: Use when writing or reviewing Rust in this repo. Typed errors, tokio hygiene for 24/7 services, ownership rules, and offline test seams.
---

# Rust conventions (AAHC)

## Errors
- Library/internal modules: one `thiserror` enum per boundary; variants carry the data the caller needs. No `Box<dyn Error>` in public APIs.
- Binary edge (`main`, HTTP handlers): `anyhow::Result` with `.context("what was being attempted")`.
- `unwrap`/`expect` only in tests, or for an invariant that is provably upheld — and the message must say why.
- Domain errors are part of the contract and must keep the Python semantics: `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED`, `UNKNOWN_CLUSTER`.
- Never turn a recoverable error into a panic to satisfy a signature. Use `?`.
- A handler must never return a raw 500 from a panic; map to a typed error response.

## Secrets
- API keys come only from the environment. Log the variable *name*, never the value.
- Do not derive `Debug` on structs that hold keys — that is a leak. Implement `Debug` manually or keep the key in a wrapper whose `Debug` prints `<redacted>`.
- Never put a key in an error message, a URL, or a test fixture.

## Async / tokio
- One runtime: tokio. Do not mix in async-std or smol.
- Never block the runtime: use `tokio::task::spawn_blocking` for sync I/O and CPU work, `rayon` for data-parallel CPU.
- No unbounded channels on ingress paths; use bounded `mpsc` so backpressure exists.
- Every network call and child process gets a timeout (`tokio::time::timeout`). No "it will finish eventually".
- Cancellation: one `CancellationToken` per unit of work, cloned into child tasks; long loops check it. Shutdown joins tasks (JoinSet) — no detached tasks left behind.
- `std::sync::Mutex` unless the guard must be held across `.await`; then `tokio::sync::Mutex`.
- Idle means no task: employees are database rows (NFR-PRF-01). Never spawn one loop per configured employee.

## Ownership and data
- Borrow in internals (`&str`, `&[u8]`), own at boundaries.
- Clone cheap handles (`Arc`), not data.
- Multi-table writes go in one transaction; a partial deploy is a bug.
- Keep SQL in one data module; handlers do not embed queries.

## Testing
- Offline by construction: inject the transport (trait + fake) instead of calling the network.
- One behaviour per test; the name states the behaviour, not the function name.
- Assert the typed error variant, not the message string.
- A test that needs a real external service must be marked and skipped by default.

## Gates
- `scripts/check.sh` must stay green: fmt clean, `clippy -D warnings`, tests pass.
- Clippy warnings are errors, not suggestions.
