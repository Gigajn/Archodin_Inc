# 04: Turn qualifying experience into pending Skills

**What to build:** Successful experience or an owner correction produces a traceable Skill proposal, never an automatically installed capability.

**Blocked by:** basileia-milestone-1#06, basileia-milestone-4#03, basileia-milestone-4#04, basileia-milestone-4#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] A successful run with at least five eligible Tool calls proposes a new/refined Skill; a shorter run alone does not qualify, while an owner correction independently can. Counting and suppression policies require approval before implementation.
- [ ] Authored proposals pass the same static-plus-LLM review, pending Role queue, exact-content owner approval and versioning as acquired Skills; rejection is visible.
- [ ] Approval never enables scripts implicitly; duplicate retries/corrections do not duplicate installation or flood identical pending proposals, and no extra Tool calls are scheduled merely to reach the trigger.

**Goals coverage:** 23, 25, 42–49.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-5. Owner must approve eligible five-Tool-call counting (failed calls, retries, concurrency and subgraphs), correction attribution, repeated-run/deduplication suppression and trigger opt-out; provide first-demo, reviewed Skill, separate script authorization and versioning prerequisite evidence. Keep the requested five-call trigger, not an invented replacement.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
