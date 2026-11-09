# 02: Cancel and bound queued/running work

**What to build:** A caller cancels a Job or observes a Step/deadline stop with a stable terminal/pause outcome and owned-task cleanup.

**Blocked by:** basileia-milestone-1#01, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Queued and running cancellation, repeated cancellation and timeout produce specified outcomes without detached work.
- [ ] Step counting semantics are explicit and later child/subgraph work cannot silently reset the configured allowance.
- [ ] State/events identify local guarantees and completed external effects; deterministic tests and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must confirm run transitions, queued/running cancellation and cleanup guarantees, Step counting and deadline semantics including future child work; provide minimal headless-Job prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
