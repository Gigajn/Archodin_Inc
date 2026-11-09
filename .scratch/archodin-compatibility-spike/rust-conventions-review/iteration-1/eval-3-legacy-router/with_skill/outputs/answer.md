# Legacy router maintenance plan

## Scope and compatibility baseline

Maintain the existing AAHC Python-parity router as application code; do not migrate it or introduce application policy into Archodin's generic Provider APIs. ADRs 0001–0002 preserve that boundary and leave legacy migration undecided. This proposal does not authorize a legacy ticket or implementation.

Before editing, obtain the approved maintenance ticket, existing public router contract and Python oracle. Record a parity matrix for `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED` and `UNKNOWN_CLUSTER`: exact triggering conditions, precedence when conditions overlap, routing/fallback order, and public response shape, including status and fields where applicable. Their names alone do not establish their meanings. These artifacts were not inspected here, so those details remain unresolved.

## Error handling

- Introduce or refine an application-owned typed router error enum with variants corresponding to verified failure categories and only the actionable, non-secret data callers need. Reuse existing dependencies and error conventions; add none for this maintenance.
- Centralize conversion from typed failures to the established legacy responses at the application boundary. Preserve verified codes, status, serialization and any contractually observable message text. Keep vendor SDK types out of public contracts and legacy codes out of generic Provider errors.
- Propagate recoverable failures rather than panicking. Retain operational context internally without exposing credential-bearing upstream messages, URLs or debug output. Use the application's configured secret boundary; fixtures contain only synthetic credentials.
- Preserve retry, cooldown and fallback behavior. If deadlines or cancellation need maintenance, establish existing semantics first; a timeout must not silently change the legacy result. Explicitly stop/join owned tasks and terminate/reap owned child processes where applicable.

## Offline characterization and regression tests

Work one behavior at a time: capture an oracle-backed expectation, demonstrate the test failure for the intended change, then make the smallest repair.

Exercise the public router through an injected scripted transport. Cover each applicable legacy code, overlapping-condition precedence, successful routing and fallback, and adapter failure mapping. Assert typed variants at the Rust contract and exact compatibility responses at the application boundary—not private call structure or incidental internal error prose. Preserve externally contracted prose where required.

Use recorded, sanitized protocol fixtures to verify real adapter mapping; a generic fake alone cannot establish it. Control clocks and synchronize tasks for cooldown, deadlines, cancellation and cleanup tests rather than sleeping. Default tests require no keys, network or inference spend; live tests remain explicitly opt-in.

## Acceptance

After authorized implementation, run focused offline tests and `./scripts/check.sh`; record actual gate output in the ticket before completion. No tests, oracle comparisons or gates were executed for this proposal.
