# 05: Attach bounded inputs and inspect usage

**What to build:** The owner submits permitted files/links/repository references and reviews provenance plus available usage/cost for the resulting Job.

**Blocked by:** basileia-milestone-1#04, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Inputs enforce agreed size/type/path and network policies; a repository link is not permission to write remotely.
- [ ] Call/attempt usage is attributable to the Job/Agent; unknown cost remains unknown rather than zero, and no unapproved paid route is selected.
- [ ] Artifact and log retrieval are bounded; private files and secrets do not appear in prompts or dashboard output accidentally.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-1. Owner must approve attachment size/type/path, link/repository and network policies plus explicit Provider/spend permissions; provide authenticated-Agora prerequisite evidence and a plan for attributable usage with unknown cost kept explicit.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
