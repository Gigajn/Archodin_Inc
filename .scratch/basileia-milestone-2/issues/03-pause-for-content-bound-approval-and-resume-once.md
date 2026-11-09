# 03: Pause for content-bound approval and resume once

**What to build:** A risky proposed action waits durably for owner approval; allow/deny decisions work through the headless interface and Agora.

**Blocked by:** basileia-milestone-2#01, basileia-milestone-1#04, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Pending/denied/stale or changed-content decisions cannot execute; duplicate valid decisions are idempotent.
- [ ] Restart and frontend reconnect preserve the pending request/outcome without holding a request open indefinitely.
- [ ] Approval policy is generic in core and application-owned in Basileia; gate and no-action-on-denial evidence are recorded.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must approve the approval lifecycle, expiry, content/version binding and run transitions; provide durable recovery and authenticated-Agora prerequisite evidence, including pending-request persistence and denial behavior.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
