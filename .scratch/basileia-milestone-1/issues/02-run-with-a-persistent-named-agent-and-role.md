# 02: Run with a persistent named Agent and Role

**What to build:** The owner configures a named Basileia Agent/Role and explicitly selected Provider account/model, then observes that identity/configuration on subsequent Jobs.

**Blocked by:** basileia-milestone-1#01, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Configuration survives a normal restart; private execution context is not shared with other Agents.
- [ ] Instructions and allowed Tools belong to Basileia; no employee-loop requirement or business error contract enters Archodin.
- [ ] Invalid/unsupported routes fail explicitly, credentials are redacted, and idle records do not spawn permanent tasks.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-1. Owner must confirm the exact Job/Graph configuration interface and explicit Provider account/model, credential and spend permissions; provide accepted spike and persistent-identity/private-context prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
