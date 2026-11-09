# 04: Approve script use independently of installation

**What to build:** An approved Skill can supply instructions while its embedded scripts remain disabled pending a separate owner decision.

**Blocked by:** basileia-milestone-4#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Installing or reading a Skill cannot execute its script; Agora distinguishes Skill installation from script authorization.
- [ ] Script changes or out-of-scope invocation require another decision; approved scripts still obey runtime isolation, Tool permissions, cancellation and Budgets.
- [ ] Script denial and restart/resume preserve the disabled state and emit auditable outcomes in offline Job tests.

**Goals coverage:** 47, 49, supporting 9, 35–36.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-4. Owner must approve script-authorization granularity and execution scope, including changed-content invalidation; provide candidate review/approval prerequisite evidence and approved isolation/Tool/cancellation/Budget enforcement evidence before script execution.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
