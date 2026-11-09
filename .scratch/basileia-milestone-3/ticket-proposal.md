# M3 — Approved vertical slices

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M3-01 — Execute conditional and function-based routes

**Blocked by:** M1-01, M2-02.

**Delivers:** A typed-State Graph chooses a branch and executes plain functions without an unnecessary model call.

- [ ] Branch outcomes, invalid routes and Step-limit stops are visible at the public Graph/run interface.
- [ ] State/prompt projection are distinct; a route does not implicitly inject all stored history.
- [ ] Existing linear behavior remains compatible; incremental offline tests and gate pass.

## M3-02 — Delegate through a structured fresh-context Handoff

**Blocked by:** M3-01, M2-03, M2-04.

**Delivers:** Basileia's Strategos assigns one bounded subtask to another named Agent using the requested five-section Briefing and artifact references.

- [ ] A fresh session can continue with goal, constraints, decisions, open questions and verification references; the full parent transcript is absent.
- [ ] Child work retains parent accounting/approval/cancel scope; planner output cannot create new authority.
- [ ] Strategos definitions remain private application assets; headless scripted acceptance and gate pass.

## M3-03 — Run parallel Nodes and nested Graphs with explicit merges

**Blocked by:** M3-01, M2-01, M2-02, M2-04.

**Delivers:** Independent branches/subgraphs complete concurrently and produce deterministic or explicitly resolved State outcomes across resume.

- [ ] Append/overwrite/conflict and stable ordering semantics are approved and tested against reversed completion order.
- [ ] Child/nested work shares parent Step/Budget/deadline/cancel limits and checkpoint/reconciliation semantics rather than resetting them.
- [ ] Invalid composition and failed branch outcomes do not corrupt accepted State; offline tests and gate pass.

## M3-04 — Evaluate a Panel with an application Qadi

**Blocked by:** M3-02, M3-03, M2-05.

**Delivers:** Multiple Agents assess one task; Qadi chooses or combines eligible semantic content with traceable reasons.

- [ ] Scripted competing answers and judge failure yield specified results, not last-writer-wins.
- [ ] Judgment cannot modify permissions, Budget accounting or approval; quality handling does not trigger technical model fallback.
- [ ] Panel/judge work is bounded, attributable and private to Basileia configuration; acceptance and gate pass.

## M3-05 — Retrieve bounded history and enforce context policy

**Blocked by:** M3-02, M3-03.

**Delivers:** An authorized receiver retrieves selected historical evidence within explicit limits, and phase changes use compact artifacts rather than growing a permanent prompt.

- [ ] Pagination/search/projection respect Agent/Job scope; denied access and truncated results remain visible.
- [ ] Model-specific headroom and smaller fallback windows are handled explicitly; thresholds are configurable rather than a universal 40% guarantee.
- [ ] Tests measure which fixtures enter the request, not hidden model reasoning; accounting includes summarization/retrieval work and the gate passes.

## M3-06 — Demonstrate nested company delegation in Agora

**Blocked by:** M3-04, M3-05.

**Delivers:** Nested Strategoi delegate a bounded Job, inspect supporting work and return a traceable final result through the shared run tree.

- [ ] A scripted nested run exercises fresh-context handoffs, parallel outcomes and an eligible judgment without unrestricted fan-out.
- [ ] Agora and headless inspection agree on child progress, decisions, usage and failure/cancel outcomes.
- [ ] Agent definitions remain outside Archodin and mandatory controls remain effective; walkthrough and gate evidence are recorded.

## Decisions required before readiness

The public run seam and Agora acceptance surface are approved for planning. Confirm merge eligibility/order, nested Step semantics and context policy before affected tickets become ready. Dependency edges follow capability needs, not an artificial requirement to finish every earlier milestone first.
