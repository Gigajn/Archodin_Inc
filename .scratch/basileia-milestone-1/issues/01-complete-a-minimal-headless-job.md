# 01: Complete a minimal headless Job

**What to build:** A bounded linear Graph accepts typed input, executes a scripted step and returns final State and run events to a headless caller.

**Blocked by:** archodin-compatibility-spike#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Success, typed failure and the Step limit are observable without Agora or proprietary agents.
- [ ] In-memory checkpoints reflect completed steps; restart recovery is not claimed.
- [ ] Local library ownership/dependencies are inspectable and contain no Basileia agent assets; regression gate and incremental tests pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-1. Owner acceptance of a positive archodin-compatibility-spike#05 recommendation, including unresolved limits, and confirmation of the exact Job/Graph interface are required; a no-go needs an explicit owner decision, not automatic implementation.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
