# 06: Run Flows and apply Disciplines safely

**What to build:** The owner deliberately starts an approved Flow and sees its fixed steps use relevant approved Disciplines through Job execution and Agora.

**Blocked by:** basileia-milestone-4#01, basileia-milestone-4#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] A Flow can use Disciplines but attempted Flow-to-Flow invocation is rejected deterministically, including indirect references.
- [ ] Discipline selection cannot grant Tool authority, bypass approvals or exceed existing Step/Budget controls.
- [ ] Events/results identify the applied Skill versions and demonstrate that an ordinary task does not implicitly start a Flow.

**Goals coverage:** 10, 50, 61–62.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-4. Owner must confirm supported Skill metadata compatibility and review failure/retry policy for Flow/Discipline versions; provide local Skill resolution and reviewed-content-approval prerequisite evidence, with agreed indirect Flow recursion rejection and existing runtime-limit enforcement.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
