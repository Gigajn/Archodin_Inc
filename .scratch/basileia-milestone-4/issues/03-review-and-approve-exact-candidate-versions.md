# 03: Review and approve exact candidate versions

**What to build:** Vigil findings and Codex's pending queue allow the owner to reject or approve reviewed Skills individually or by Role group.

**Blocked by:** basileia-milestone-4#01, basileia-milestone-2#04, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Static review precedes scripted LLM review; rejection, unavailable review and reasons appear in Agora and headless events without installation.
- [ ] Both downloaded and authored/changed candidates require approval bound to content and target Role; editing after review invalidates approval.
- [ ] Group approval lists exact versions and findings; replaying approval or resuming a Job cannot duplicate installation.

**Goals coverage:** 41, 43–46, 61–62.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-4. Owner must approve Skill metadata/content identity, trust/strict-review rules and review retry/failure policy; provide local Skill and Budget prerequisites with inherited content-bound approval evidence. Confirm exact-version/Role group review and replay handling before readiness.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
