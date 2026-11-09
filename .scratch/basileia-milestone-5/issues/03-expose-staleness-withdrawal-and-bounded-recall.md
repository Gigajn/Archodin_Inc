# 03: Expose staleness, withdrawal and bounded recall

**What to build:** The owner can inspect freshness, supersede or withdraw knowledge, then observe changed retrieval behavior on the next Job.

**Blocked by:** basileia-milestone-5#02, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] A scripted stale or superseded source is identified under the agreed policy and is not presented as an unqualified current fact.
- [ ] Correction/withdrawal changes subsequent results, preserves an appropriate audit trail and respects the owner-approved deletion/retention policy.
- [ ] Retrieval limits and unavailable evidence produce visible bounded outcomes rather than private-session fallback or unlimited context expansion.

**Goals coverage:** 23, 27–29, 49.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-5. Owner must approve staleness thresholds, conflict handling, sensitive-data deletion/retention and bounded retrieval limits; provide curation prerequisite evidence and define the withdrawal/audit behavior without treating retrieved facts as automatically fresh.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
