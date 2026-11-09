# 04: Enforce atomic Budget reservation and settlement

**What to build:** Agent/Job allowances are reserved before execution, settled from attributable usage, and paused for an owner decision when insufficient.

**Blocked by:** basileia-milestone-1#05, basileia-milestone-2#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Concurrent requests cannot reserve the same remaining allowance; children, retries and supporting calls count to the originating Job.
- [ ] Unused reservations are released/reconciled after cancellation/restart; duplicate settlement cannot double-charge.
- [ ] Unknown price/usage policy is approved explicitly; unbounded cost cannot be advertised as a monetary hard cap. Offline adversarial tests and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must approve monetary reservation/settlement and exhaustion decisions, conservative pricing/usage-gap treatment and reconciliation policy; provide usage-input and content-bound-approval prerequisite evidence. Unknown/unbounded cost cannot become a claimed hard cap.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
