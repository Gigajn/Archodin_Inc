# M5 — Approved ticket publication

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M5-01 — Retrieve scoped Akasha knowledge with citations

**Blocked by:** M1-02, M3-05.

**Observable delivery:** A Job retrieves bounded Role/project knowledge, and Agora shows the excerpts and sources actually used without exposing another Agent's conversation.

- [ ] Two Roles and two projects demonstrate allowed facts, denied cross-scope retrieval and explicit Role-wide facts; similarity does not override access scope.
- [ ] Headless results and Agora show provenance, revision and empty/truncated retrieval outcomes with deterministic local fixtures.
- [ ] Named Agents retain separate private sessions across Jobs; only material promoted under the agreed policy can enter shared retrieval.

**Goals coverage:** 21, 23, 27, 29, 61–62.

## M5-02 — Curate and deduplicate promoted knowledge

**Blocked by:** M5-01.

**Observable delivery:** Codex condenses eligible material into inspectable Akasha entries, and the owner can compare, correct or reverse a curation result.

- [ ] Duplicate fixture entries yield concise retrieval while retaining source references and curation history rather than erasing attribution.
- [ ] Conflicting claims remain visible and require the agreed resolution policy; an LLM suggestion cannot silently rewrite authoritative evidence.
- [ ] Scope/promotion controls reject private or unauthorized material; curation cannot change core code, Providers or Tool permissions.

**Goals coverage:** 27–29, 49.

## M5-03 — Expose staleness, withdrawal and bounded recall

**Blocked by:** M5-02.

**Observable delivery:** The owner can inspect freshness, supersede or withdraw knowledge, then observe changed retrieval behavior on the next Job.

- [ ] A scripted stale or superseded source is identified under the agreed policy and is not presented as an unqualified current fact.
- [ ] Correction/withdrawal changes subsequent results, preserves an appropriate audit trail and respects the owner-approved deletion/retention policy.
- [ ] Retrieval limits and unavailable evidence produce visible bounded outcomes rather than private-session fallback or unlimited context expansion.

**Goals coverage:** 23, 27–29, 49.

## M5-04 — Turn qualifying experience into pending Skills

**Blocked by:** M1-06, M4-03, M4-04, M4-05.

**Observable delivery:** Successful experience or an owner correction produces a traceable Skill proposal, never an automatically installed capability.

- [ ] A successful run with at least five eligible Tool calls proposes a new/refined Skill; a shorter run alone does not qualify, while an owner correction independently can. Counting and suppression policies require approval before implementation.
- [ ] Authored proposals pass the same static-plus-LLM review, pending Role queue, exact-content owner approval and versioning as acquired Skills; rejection is visible.
- [ ] Approval never enables scripts implicitly; duplicate retries/corrections do not duplicate installation or flood identical pending proposals, and no extra Tool calls are scheduled merely to reach the trigger.

**Goals coverage:** 23, 25, 42–49.

## M5-05 — Demonstrate learning reuse without session leakage

**Blocked by:** M5-03, M5-04, M4-06.

**Observable delivery:** A second Agent of the same Role completes a subsequent Job using curated knowledge and an approved learned Skill, with an Agora explanation of what was reused.

- [ ] A two-Job offline scenario identifies cited Akasha revisions and approved Flow/Discipline versions while excluding the first Agent's private conversation.
- [ ] Rejected, withdrawn or unauthorized material is absent; rolled-back Skills follow the selected version and do not inherit broader script permissions.
- [ ] An unrelated Role/project cannot gain knowledge or Skills through the learning path, and runtime approvals, Budgets and Tool authority remain unchanged.

**Goals coverage:** 23–29, 42, 48–50, 61–62.

## Decisions required before readiness

Confirm knowledge promotion consent, Role/project scope, sensitive-data retention/deletion, freshness/conflict policy, retrieval limits and five-call semantics (failed calls, retries, concurrent calls, subgraphs). Confirm correction attribution, deduplication and opt-out without replacing the requested proposed trigger. Slice size and actual upstream blockers are approved for publication; readiness still requires owner decisions and prerequisite evidence; no additional ADR, framework or protocol is selected by this document.
