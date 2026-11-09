# Generic Provider contract

## Scope and API

Publish an MIT-licensed, integration-neutral core with no required third-party dependencies. No integration SDK, transport, credentials scheme, or provider-specific payload is selected. Adapters remain separate modules or crates; review their licenses when chosen.

For an asynchronous, statically dispatched contract, propose Rust 1.75+:

```rust
pub trait Provider: Send + Sync {
    type Request: Send;
    type Response: Send;
    type Error: std::error::Error + Send + Sync + 'static;

    fn execute(
        &self,
        request: Self::Request,
    ) -> impl std::future::Future<
        Output = Result<Self::Response, Self::Error>,
    > + Send;
}
```

Requests and responses own their boundary data; internal adapter code may borrow. Consumers depend on `P: Provider`, allowing a fake wherever a real adapter would be used. Associated types deliberately permit different integrations without inventing a universal payload. Interchangeable adapters must agree on request and response types. This contract is not dyn-compatible; add an explicit boxed-future abstraction only if heterogeneous runtime registration becomes a requirement.

`execute` represents one attempt. Construction validates local configuration and performs no hidden I/O. No implicit retries, spawned background tasks, global runtime, or environment reads. Async alone does not guarantee nonblocking behavior: adapters must document and isolate blocking work.

## Errors and operational semantics

Each adapter exposes a concrete typed error enum, implementing `Display` and `Error`, with structured variants such as invalid configuration, unsupported operation, timeout, rate limiting with optional retry delay, and backend failure with a retained typed source. Include only variants the adapter can actually produce. Use handwritten implementations in the dependency-free core; `thiserror` can be an adapter choice. Avoid public `Box<dyn Error>`, `anyhow`, panic-based recovery, and secret-bearing diagnostics.

The snapshot’s `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED`, and `UNKNOWN_CLUSTER` describe its application, not a generic provider; introduce them only if that domain is explicitly required.

Dropping the returned future requests local cancellation, not remote rollback. Adapters must document side-effect uncertainty. Do not automatically retry writes without an explicit idempotency contract. Network adapters must enforce finite deadlines; a Tokio adapter can use Tokio timeouts without imposing Tokio on the core. Never expose secrets through errors or debugging.

## Testing and unresolved decisions

Use deterministic offline fakes and injected transports. Test one behavior at a time: successful response, typed failure propagation, timeout, rate-limit metadata, cancellation cleanup, and no implicit retry. Assert error variants, not text. Compile-check promised `Send` properties. Keep optional live-service tests explicitly ignored by default.

Confirm MSRV, actual operations, runtime integration, and deadline configuration before implementation. This is a design proposal; no build, tests, dependency resolution, or network validation was performed.
