# Milestone 2 — Controlled and recoverable execution

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

A successful local demonstration is not sufficient for long-running operation. The owner must be able to cancel, approve, recover and bound work without duplicate actions, silent spending or unsafe Provider fallback.

## Solution

Extend public Job execution with durable checkpoints, explicit run transitions, generic approvals, usage/Budget accounting, model-chain policies and authenticated operational controls. Basileia supplies account configuration and Agora policy; Archodin provides reusable contracts and enforcement primitives. Prove the failure paths offline before enabling unattended use.

## User Stories

1. As the owner, I want completed steps saved durably, so that a process restart need not lose accepted progress.
2. As the owner, I want recovery to reconcile external actions, so that resume does not blindly repeat a successful side effect.
3. As the owner, I want cancellation of queued/running work, so that unwanted work stops within defined local guarantees.
4. As the owner, I want Step limits and deadlines, so that loops and stalled operations remain bounded.
5. As the owner, I want risky work paused for an explicit decision, so that no Agent prompt can grant itself authority.
6. As a library user, I want approval independent of Agora, so that another frontend can implement the same decision contract.
7. As the owner, I want approvals tied to the proposed action/version, so that stale consent cannot approve changed work.
8. As the owner, I want Agent and Job Budgets, so that concurrency and child work cannot each spend the same remaining allowance.
9. As the owner, I want budget exhaustion to pause and request a decision, so that overspending is not implicit.
10. As the owner, I want multiple Provider accounts and Role/Agent Model chains, so that configured technical failures can use an allowed alternative.
11. As the owner, I want quality disagreements kept separate from technical failover, so that fallback is predictable.
12. As the owner, I want usage and costs aggregated by Agent, Job and Department, so that supporting calls and retries are visible.
13. As the owner, I want authenticated inspection/control after reconnect, so that remote use does not bypass local safety.
14. As a maintainer, I want crash, concurrency and denial paths reproducible offline, so that reliability does not depend on lucky timing.

## Implementation Decisions

- Proposed: define valid run/approval/Budget transitions and recovery behavior before implementing them. An approval wait is persisted, not an indefinitely blocked HTTP request.
- Use durable checkpoint storage compatible with current needs; choose migration/version handling and storage adapter explicitly. Do not select Postgres or an event-sourcing architecture by implication.
- Persist action identities and outcomes where side effects occur; ambiguous outcomes require reconciliation. A checkpoint alone cannot promise exactly-once remote execution.
- Cancellation propagates to owned work with bounded cleanup. Document irreversible/already-started external effects and remote compute limitations.
- Reserve Budget atomically before parallel spend, settle actual attributable usage and release unused reservation. Count retries and supporting agents under the originating Job. Missing prices/usage require an explicit policy; do not claim a strict cap when cost cannot be conservatively bounded.
- Classify authentication/configuration/request failures separately from transient transport/rate-limit/timeouts. Honor account cooldown, bounded retry and capability requirements; cancellation and policy denial are not fallthrough routes.
- Basileia controls Role defaults and per-Agent overrides. Library contracts do not embed company-specific routing clusters, employee policy or Qadi.
- Security decisions are runtime checks, not LLM instructions. Redact secrets in persisted events and reconnectable logs; keep one-owner scope and explicit remote transport requirements.

## Testing Decisions

- Proposed primary seam: Job submission/control and emitted events/final State, using fake clock, scripted Providers, durable test store and fake action adapters.
- Approval is an additional real interface: tests substitute approve/deny/pending/stale decisions and prove no action occurs without valid consent. Agora acceptance verifies the same run transitions, not a separate scheduler.
- Exercise crash before/after effect and checkpoint, duplicate decision, simultaneous reservations, exhaustion, reconciliation, failed account, timeout, unsupported fallback and unknown-cost policy. Assert invariants rather than private table shape.
- Verify unauthenticated access is denied, reconnect shows consistent history, secrets remain redacted and existing behavior remains green. TDD and per-ticket repository gate remain mandatory.

## Out of Scope

Planning quality, Panel/Qadi judgments, long-term learning, Skill acquisition, HR and live financial/trading actions. Cancellation does not promise reversal of completed actions; fake external success does not certify a live integration.

## Further Notes

Covers Goals 6–9, 30–37 and 62, supports 4–5, 23–24 and later autonomy. Sources: `Goals.md` and accepted ADRs; proposals are in sibling `ticket-proposal.md`. Approval expiry, monetary reservation policy, pricing/usage gaps, checkpoint schema compatibility and secure remote-access method remain decisions required before affected tickets are ready.
