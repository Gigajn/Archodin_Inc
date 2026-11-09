# 03: Produce and test an isolated local software change

**What to build:** A small software Job changes a designated fixture project and produces local patch/test artifacts within an isolated environment.

**Blocked by:** basileia-milestone-1#02, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Demonstrate an allowed edit/test and denied filesystem/credential/host-runtime escapes; the isolation design must be approved before enabling generated-code execution.
- [ ] Tool permissions, deadlines and bounded logs apply; no arbitrary host shell, push, deploy or financial action is available.
- [ ] Scripted Provider tests prove local artifacts and failure reporting without external model calls.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-1. Owner must approve the isolation implementation, allowed filesystem/network/credential authority and escape-check design before generated-code execution; provide identity/configuration prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
