# Legacy Python-parity router: focused maintenance plan

**Scope:** Preserve observable behavior; do not redesign routing or introduce dependencies. This proposal uses only the assigned conventions snapshot. Router implementation, Python behavior, and existing tests were not inspected, so the details below require characterization before implementation.

## 1. Establish the compatibility contract

Before changing code, document the existing conditions and precedence for `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED`, and `UNKNOWN_CLUSTER`. Capture response status, payload shape, exact public error codes, routing order, fallback behavior, and cooldown transitions. Include overlapping conditions: do not assume whether an unknown cluster or absent keys takes precedence. Preserve current retry counts and timing rather than introducing a new policy.

## 2. Contain error handling at existing boundaries

Use a `thiserror` enum at the router boundary, with explicit variants for the four domain outcomes and only the structured data callers need. Reuse an existing equivalent type if present. Keep transport failures distinguishable internally so existing fallback and aggregate-failure behavior remains intact.

Propagate recoverable failures with `?`; remove panic-based handling only after its replacement behavior is specified. Do not expose `Box<dyn Error>` from public APIs. At the HTTP boundary, explicitly map typed outcomes to the existing Python-compatible status and body. Keep contextual `anyhow` errors at binary edges where already appropriate, without replacing structured client responses or leaking internal diagnostics.

Never include key values in errors, URLs, fixtures, or logs. Use redacted debug formatting for key-bearing structures.

## 3. Add a minimal offline seam

Inject transport through a trait and scripted fake; keep production routing decisions in the router, not the fake. Record attempted routes and return controlled responses or failures. Control time through the smallest existing clock seam or supported Tokio test facilities, without adding dependencies.

Network operations should have bounded timeouts. First characterize current timeout behavior; treat any new observable timeout policy as a separately approved compatibility change.

## 4. Proceed test-first, one behavior at a time

Cover each domain outcome, overlapping-condition precedence, successful fallback after failure, all-attempts-failed behavior, and cooldown expiry. Assert typed variants internally; assert exact public codes and response shapes at the compatibility boundary. Verify attempt order and state changes, not merely final results. Fixtures contain no secrets and require no external service or environment credentials.

For each maintenance change, establish a failing regression test before the smallest fix. Before acceptance, require `./scripts/check.sh` to pass. No implementation, builds, or tests were performed for this evaluation.
