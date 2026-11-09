# M2 — Approved vertical slices

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M2-01 — Resume a durable Job without blind effect replay

**Blocked by:** M1-01.

**Delivers:** A run restarted after a checkpoint resumes accepted progress and reconciles side-effect identities rather than blindly repeating work.

- [ ] Crash/restart fixtures prove durable State and explicit invalid/incompatible checkpoint outcomes.
- [ ] Before/after-effect crash cases identify successful, failed and ambiguous actions; duplicated submission cannot duplicate an accepted local action.
- [ ] Define storage/version/action identity policy before implementation; record offline tests and gate.

## M2-02 — Cancel and bound queued/running work

**Blocked by:** M1-01.

**Delivers:** A caller cancels a Job or observes a Step/deadline stop with a stable terminal/pause outcome and owned-task cleanup.

- [ ] Queued and running cancellation, repeated cancellation and timeout produce specified outcomes without detached work.
- [ ] Step counting semantics are explicit and later child/subgraph work cannot silently reset the configured allowance.
- [ ] State/events identify local guarantees and completed external effects; deterministic tests and gate pass.

## M2-03 — Pause for content-bound approval and resume once

**Blocked by:** M2-01, M1-04.

**Delivers:** A risky proposed action waits durably for owner approval; allow/deny decisions work through the headless interface and Agora.

- [ ] Pending/denied/stale or changed-content decisions cannot execute; duplicate valid decisions are idempotent.
- [ ] Restart and frontend reconnect preserve the pending request/outcome without holding a request open indefinitely.
- [ ] Approval policy is generic in core and application-owned in Basileia; gate and no-action-on-denial evidence are recorded.

## M2-04 — Enforce atomic Budget reservation and settlement

**Blocked by:** M1-05, M2-03.

**Delivers:** Agent/Job allowances are reserved before execution, settled from attributable usage, and paused for an owner decision when insufficient.

- [ ] Concurrent requests cannot reserve the same remaining allowance; children, retries and supporting calls count to the originating Job.
- [ ] Unused reservations are released/reconciled after cancellation/restart; duplicate settlement cannot double-charge.
- [ ] Unknown price/usage policy is approved explicitly; unbounded cost cannot be advertised as a monetary hard cap. Offline adversarial tests and gate pass.

## M2-05 — Run an allowed Model chain across configured accounts

**Blocked by:** M1-02, M2-02, M2-04.

**Delivers:** A technical failure follows an owner-configured compatible account/model alternative with bounded retries/cooldown and complete usage attribution.

- [ ] Role defaults and Agent overrides, multiple accounts, transient failures and rate-limit cooldown are observable through run results.
- [ ] Invalid credentials/request, unsupported capability, cancellation and policy denial follow explicit classifications, not indiscriminate fallback.
- [ ] No quality-based switching or unapproved paid route is introduced; unknown accounting stays explicit. Contract tests and gate pass.

## M2-06 — Verify authenticated remote control and recovery

**Blocked by:** M1-04, M2-01, M2-02, M2-03, M2-04, M2-05.

**Delivers:** An owner reconnects to Agora, inspects attributable Job/Agent/Department usage and operates cancel/approval/recovery consistently with headless execution.

- [ ] Authentication and session/control protections apply to reads, streaming events and writes; insecure remote exposure is refused under the approved deployment policy.
- [ ] Restart/concurrent approval/Budget/cancel walkthroughs preserve invariant outcomes and bounded logs without secrets.
- [ ] Operational limits, unknown spend and recovery limitations are visible; the gate and offline acceptance evidence are recorded.

## Decisions required before readiness

Confirm run transition/step semantics, checkpoint compatibility, approval lifecycle, conservative Budget accounting, fallback classes and remote-access policy. This proposal does not silently approve those trade-offs or require every M2 slice before later work that has narrower true prerequisites.
