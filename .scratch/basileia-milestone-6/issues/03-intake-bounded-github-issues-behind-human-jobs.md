# 03: Intake bounded GitHub issues behind human Jobs

**What to build:** An approved Department creates Jobs from eligible issues within owner-defined limits while human-origin Jobs receive scheduling priority.

**Blocked by:** basileia-milestone-6#02, basileia-milestone-2#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Scripted repeated polls and restart produce at most one active intake decision/Job per agreed source identity; rejected/ineligible issues show reasons.
- [ ] Scope, queue/concurrency quotas and Budget checks pause intake deterministically; issue text cannot enlarge Charter permissions.
- [ ] With human and issue Jobs queued, the human Job is selected first; tests document the agreed behavior for already-running automatic work and cancellation.

**Goals coverage:** 57–58, supporting 6, 35–37.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-6. Owner must approve GitHub credentials/scopes and repository eligibility checks, intake quotas/source identity, human priority versus preemption/cancellation and restart reconciliation; provide Charter and model-chain prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
