# 01: Resume a durable Job without blind effect replay

**What to build:** A run restarted after a checkpoint resumes accepted progress and reconciles side-effect identities rather than blindly repeating work.

**Blocked by:** basileia-milestone-1#01, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Crash/restart fixtures prove durable State and explicit invalid/incompatible checkpoint outcomes.
- [ ] Before/after-effect crash cases identify successful, failed and ambiguous actions; duplicated submission cannot duplicate an accepted local action.
- [ ] Define storage/version/action identity policy before implementation; record offline tests and gate.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must select checkpoint storage/adapter, schema compatibility and migration/version policy, action identities and reconciliation outcomes; provide minimal headless-Job prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
