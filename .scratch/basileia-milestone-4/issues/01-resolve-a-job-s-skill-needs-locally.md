# 01: Resolve a Job's Skill needs locally

**What to build:** A submitted Job exposes Ronin's required/missing capabilities and matches already approved Role Skills in Agora and the headless result.

**Blocked by:** basileia-milestone-1#02, basileia-milestone-2#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] A scripted Job uses a local approved match without search; a miss is visible before work assignment rather than silently fabricating a Skill.
- [ ] Skill kind, version and Role scope are inspectable; an unrelated Role cannot consume an unshared Skill.
- [ ] Ronin is application behavior; generic library execution works without Agora or company assets.

**Goals coverage:** 24–26, 38, 61–62.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-4. Owner must confirm supported Skill metadata compatibility and Role/shared-scope access policy; provide named-identity and content-bound-approval prerequisite evidence. Ronin and company assets remain private application behavior.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
