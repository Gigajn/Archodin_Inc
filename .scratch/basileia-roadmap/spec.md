# Basileia and Archodin — Remaining implementation roadmap

**Status:** approved planning baseline — the owner accepted the complete roadmap, testing seams and slice breakdown. Milestone 0 is complete. Spike and supporting work have individual issues; all 35 later milestone issues are published as needs-info until their explicit policy/readiness conditions and prerequisites are resolved. Planning approval is not runtime completion.

## Problem Statement

The owner needs a complete path from the accepted Milestone 0 product boundary to a usable, controlled AI-company application. Planning only Provider compatibility would leave Jobs, Agora, delegation, Skills, learning and organizational growth without an implementation plan. Conversely, declaring every future policy settled would create misleading ready tickets.

## Solution

Plan Milestones 1–6 as verifiable capability increments, with the accepted compatibility spike as Milestone 1's prerequisite. Each milestone has its own draft spec and proposed vertical slices. Maintain explicit story coverage, real capability dependencies and acceptance evidence. Publish only approved, actionable tickets; keep unresolved runtime policies visible rather than silently inventing ADRs.

## User Stories

1. As the owner, I want every remaining Goal assigned to a milestone, so that a small spike is not confused with completion of the product.
2. As the owner, I want one complete local software Job first, so that development produces an observable result early.
3. As a library user, I want generic Rust execution independent of Basileia, so that Archodin is reusable under MIT.
4. As the owner, I want persistent named Agent/Role configuration and private bounded sessions, so that identities survive without growing permanent prompts.
5. As the owner, I want an operational Agora dashboard, so that I can submit, inspect and control work remotely with authentication.
6. As the owner, I want recovery, cancellation, approval and Budget enforcement, so that failures do not silently duplicate work or spend.
7. As the owner, I want compatible configured Provider fallback with honest usage accounting, so that technical failure does not bypass my policies.
8. As the owner, I want Strategos delegation, Briefings and bounded history retrieval, so that work can be distributed without transcript flooding.
9. As a Graph author, I want routing, parallelism, nested Graphs and explicit merges, so that composition is deterministic where possible.
10. As the owner, I want Panel/Qadi judgments restricted to eligible semantic outcomes, so that runtime invariants are not decided by an LLM.
11. As the owner, I want Skill discovery, review and exact-version approval with separate script authority, so that new knowledge cannot authorize itself.
12. As the owner, I want Role-shared capabilities and scoped Akasha, so that learning is reusable without leaking private/client material.
13. As the owner, I want experience to propose evaluated Skill improvements, so that the company can improve without changing core code or permissions.
14. As the owner, I want approved staffing/Charters and bounded issue intake, so that organizational growth remains within my chosen fields and costs.
15. As the owner, I want compliant fork/branch/disclosed-PR workflows, so that future company contributions respect external maintainers.
16. As the owner, I want legacy tickets revalidated and dependency eligibility checked, so that old ready labels cannot silently override the new product direction.
17. As the owner, I want deployment and extraction-readiness evidence, so that I can later separate and push the public/private repositories manually.
18. As the owner, I want an optional later 2D office using existing events/controls, so that visualization does not delay required operation.

## Implementation Decisions

- Accepted product boundaries remain unchanged: generic Archodin library, proprietary Basileia behavior/Agora, Rust preferred, single-owner local deployment with authenticated remote access. No company agents are bundled into the public library.
- The six-milestone sequence is an approved planning baseline, not a claim that every ticket-local runtime policy has been resolved. Milestone 1 includes the compatibility prerequisite and first software demonstration; Milestones 2–6 add controlled execution, delegation, Skills, knowledge and bounded organization respectively.
- Use vertical slices with observable outcomes and incremental tests; do not turn each subsystem into an empty scaffold ticket or make UI a prerequisite for every headless contract.
- Depend on concrete prior capabilities, not completion of every earlier milestone. In particular, HR need not wait for Akasha, and local Skill matching need not wait for full nested delegation. Final acceptance still exercises all mandatory capabilities together.
- Preserve the approved spike sequence and do not silently implement its Skill maintenance during planning. Upgrade/merge Skills only for an evidenced gap with baseline comparison, review and versionable outcomes.
- Cross-feature blockers and eligibility need a separately tested execution guard before later milestone proposals are published as executable tickets. Until then, document those edges and check them manually; current script output is not authorization.
- Legacy code is a candidate for reuse, not an automatic migration target. Preserve user edits and ticket history, obtain owner-confirmed revalidation and use expand/contract if changes genuinely span incompatible contracts.
- Runtime safety details beyond Milestone 0 remain proposed. Monetary hard-cap assumptions, effect reconciliation, script scope, knowledge promotion and remote-access choices require explicit resolution before affected implementation tickets become ready.
- Default tests use fakes and local fixtures without external accounts or inference spending. Actual external smoke checks require explicit permissions and cannot be replaced by claims based only on fake success.

## Testing Decisions

- Proposed run-level seam for M1–M6: public headless Job/Graph submission, control, emitted events and final State/artifacts. Generic approval is another real interface, and Agora has an acceptance surface for the same outcomes.
- The owner-approved Provider and typed Tool seams remain in the compatibility prerequisite; do not invent a universal facade or test private SDK/database layouts to force one seam everywhere.
- Every mandatory ticket demonstrates behavior, adds one red→green case at a time where code changes, and records the repository gate before done. Skill changes use baseline/updated evaluation and owner review rather than claiming static checks are model benchmarks.
- Validate run success and denial/failure paths, concurrency/Budget invariants, durable recovery, least-authority execution, source/Role isolation and duplicate external-action handling across milestones.
- An end-to-end mandatory-release walkthrough must exercise all required capabilities with reproducible evidence. Existing 14 legacy integration tests are only the starting regression baseline; publication approval is not implementation evidence.

## Out of Scope

Implementing the roadmap during this planning task; choosing a dependency/framework before evidence; mandatory Trading/Media/CS/ML products; SaaS/multi-tenant billing; unlimited autonomous spending; unofficial consumer-subscription routes; private reasoning-trace exposure; actual repository split, remotes or push/publication by agents. Optional office work is separate from mandatory product completion.

## Further Notes

- Product authority: `Goals.md`, `GLOSSARY.md`, accepted ADRs, and `docs/agents/domain.md`.
- Roadmap/coverage index: sibling `roadmap.md`; supporting work proposals: sibling `ticket-proposal.md`.
- Each milestone spec and proposal lives under its corresponding `.scratch/basileia-milestone-N/` feature directory. The compatibility spike remains in its own feature with published approved tickets.
- Pending approval concerns scope, seams, slice granularity and blockers; it is not a new interview about already accepted product decisions. Resolve ticket-local trade-offs just in time, without disguising a blocked design decision as ready implementation.
