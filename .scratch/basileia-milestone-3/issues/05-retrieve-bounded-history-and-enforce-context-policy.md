# 05: Retrieve bounded history and enforce context policy

**What to build:** An authorized receiver retrieves selected historical evidence within explicit limits, and phase changes use compact artifacts rather than growing a permanent prompt.

**Blocked by:** basileia-milestone-3#02, basileia-milestone-3#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Pagination/search/projection respect Agent/Job scope; denied access and truncated results remain visible.
- [ ] Model-specific headroom and smaller fallback windows are handled explicitly; thresholds are configurable rather than a universal 40% guarantee.
- [ ] Tests measure which fixtures enter the request, not hidden model reasoning; accounting includes summarization/retrieval work and the gate passes.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-3. Owner must approve scoped history access, retrieval/tool-result limits, model-specific context thresholds/headroom and Briefing validation; provide Handoff and parallel/nested Graph prerequisite evidence, including fallback-window handling.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
