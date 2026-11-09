# 04: Evaluate a Panel with an application Qadi

**What to build:** Multiple Agents assess one task; Qadi chooses or combines eligible semantic content with traceable reasons.

**Blocked by:** basileia-milestone-3#02, basileia-milestone-3#03, basileia-milestone-2#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Scripted competing answers and judge failure yield specified results, not last-writer-wins.
- [ ] Judgment cannot modify permissions, Budget accounting or approval; quality handling does not trigger technical model fallback.
- [ ] Panel/judge work is bounded, attributable and private to Basileia configuration; acceptance and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-3. Owner must define eligible semantic conflicts, judgment/merge order and judge-failure outcomes, keeping runtime invariants outside Qadi authority; provide Handoff, parallel/nested Graph and permitted model-chain prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
