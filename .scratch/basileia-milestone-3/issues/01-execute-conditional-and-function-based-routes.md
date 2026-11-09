# 01: Execute conditional and function-based routes

**What to build:** A typed-State Graph chooses a branch and executes plain functions without an unnecessary model call.

**Blocked by:** basileia-milestone-1#01, basileia-milestone-2#02, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Branch outcomes, invalid routes and Step-limit stops are visible at the public Graph/run interface.
- [ ] State/prompt projection are distinct; a route does not implicitly inject all stored history.
- [ ] Existing linear behavior remains compatible; incremental offline tests and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-3. Owner must confirm routing and nested Step-count semantics and the State/prompt projection boundary; provide headless-Job and cancellation/limit prerequisite evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
