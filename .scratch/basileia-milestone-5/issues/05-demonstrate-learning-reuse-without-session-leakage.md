# 05: Demonstrate learning reuse without session leakage

**What to build:** A second Agent of the same Role completes a subsequent Job using curated knowledge and an approved learned Skill, with an Agora explanation of what was reused.

**Blocked by:** basileia-milestone-5#03, basileia-milestone-5#04, basileia-milestone-4#06, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] A two-Job offline scenario identifies cited Akasha revisions and approved Flow/Discipline versions while excluding the first Agent's private conversation.
- [ ] Rejected, withdrawn or unauthorized material is absent; rolled-back Skills follow the selected version and do not inherit broader script permissions.
- [ ] An unrelated Role/project cannot gain knowledge or Skills through the learning path, and runtime approvals, Budgets and Tool authority remain unchanged.

**Goals coverage:** 23–29, 42, 48–50, 61–62.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-5. Owner must resolve knowledge promotion consent, Role/project scope, sensitive-data retention/deletion, freshness/conflict/retrieval policy and eligible-call/correction/suppression/opt-out semantics; provide withdrawal/recall, experience-to-pending-Skill and Flow/Discipline prerequisite evidence, including rollback and private-session exclusion.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
