# 03: Run parallel Nodes and nested Graphs with explicit merges

**What to build:** Independent branches/subgraphs complete concurrently and produce deterministic or explicitly resolved State outcomes across resume.

**Blocked by:** basileia-milestone-3#01, basileia-milestone-2#01, basileia-milestone-2#02, basileia-milestone-2#04, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Append/overwrite/conflict and stable ordering semantics are approved and tested against reversed completion order.
- [ ] Child/nested work shares parent Step/Budget/deadline/cancel limits and checkpoint/reconciliation semantics rather than resetting them.
- [ ] Invalid composition and failed branch outcomes do not corrupt accepted State; offline tests and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-3. Owner must approve append/overwrite/conflict eligibility, merge ordering and nested Step/depth/fan-out semantics; provide routing, durable recovery, cancellation and Budget prerequisite evidence covering child checkpoints and reconciliation.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
