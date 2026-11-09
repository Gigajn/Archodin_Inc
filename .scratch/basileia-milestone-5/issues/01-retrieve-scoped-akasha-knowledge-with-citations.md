# 01: Retrieve scoped Akasha knowledge with citations

**What to build:** A Job retrieves bounded Role/project knowledge, and Agora shows the excerpts and sources actually used without exposing another Agent's conversation.

**Blocked by:** basileia-milestone-1#02, basileia-milestone-3#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Two Roles and two projects demonstrate allowed facts, denied cross-scope retrieval and explicit Role-wide facts; similarity does not override access scope.
- [ ] Headless results and Agora show provenance, revision and empty/truncated retrieval outcomes with deterministic local fixtures.
- [ ] Named Agents retain separate private sessions across Jobs; only material promoted under the agreed policy can enter shared retrieval.

**Goals coverage:** 21, 23, 27, 29, 61–62.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-5. Owner must approve Role-wide versus project-specific facts, promotion consent, sensitive-data retention/deletion and retrieval limits; provide named-identity and bounded-history prerequisite evidence proving private-session exclusion.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
