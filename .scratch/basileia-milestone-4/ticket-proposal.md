# M4 — Approved ticket publication

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M4-01 — Resolve a Job's Skill needs locally

**Blocked by:** M1-02, M2-03.

**Observable delivery:** A submitted Job exposes Ronin's required/missing capabilities and matches already approved Role Skills in Agora and the headless result.

- [ ] A scripted Job uses a local approved match without search; a miss is visible before work assignment rather than silently fabricating a Skill.
- [ ] Skill kind, version and Role scope are inspectable; an unrelated Role cannot consume an unshared Skill.
- [ ] Ronin is application behavior; generic library execution works without Agora or company assets.

**Goals coverage:** 24–26, 38, 61–62.

## M4-02 — Acquire candidates with trust and provenance

**Blocked by:** M4-01, M2-04, M2-05.

**Observable delivery:** The owner can inspect external candidates, origin and trust classification without any candidate becoming active.

- [ ] Scripted SearXNG-when-configured, DuckDuckGo-otherwise and GitHub responses demonstrate acquisition, unavailable/rate-limited outcomes and no mandatory login or paid search.
- [ ] Optional GitHub connection exposes its granted capabilities; every candidate retains source/revision or content identity, applicable license information, and trusted/unverified classification.
- [ ] Untrusted acquisition receives stricter proposed scrutiny; retrieval failure or Budget exhaustion cannot authorize paid fallback or installation.

**Goals coverage:** 38–41, supporting 35–37.

## M4-03 — Review and approve exact candidate versions

**Blocked by:** M4-01, M2-04.

**Observable delivery:** Vigil findings and Codex's pending queue allow the owner to reject or approve reviewed Skills individually or by Role group.

- [ ] Static review precedes scripted LLM review; rejection, unavailable review and reasons appear in Agora and headless events without installation.
- [ ] Both downloaded and authored/changed candidates require approval bound to content and target Role; editing after review invalidates approval.
- [ ] Group approval lists exact versions and findings; replaying approval or resuming a Job cannot duplicate installation.

**Goals coverage:** 41, 43–46, 61–62.

## M4-04 — Approve script use independently of installation

**Blocked by:** M4-03.

**Observable delivery:** An approved Skill can supply instructions while its embedded scripts remain disabled pending a separate owner decision.

- [ ] Installing or reading a Skill cannot execute its script; Agora distinguishes Skill installation from script authorization.
- [ ] Script changes or out-of-scope invocation require another decision; approved scripts still obey runtime isolation, Tool permissions, cancellation and Budgets.
- [ ] Script denial and restart/resume preserve the disabled state and emit auditable outcomes in offline Job tests.

**Goals coverage:** 47, 49, supporting 9, 35–36.

## M4-05 — Version, roll back and share Role capabilities

**Blocked by:** M4-03, M4-04.

**Observable delivery:** Role colleagues see approved Skill updates; the owner can inspect history, reverse a version and control Codex sharing to other Roles.

- [ ] Updates retain review/approval provenance and become available to the Role without publishing private conversations.
- [ ] An explicit shared designation and owner-approved destination scope govern cross-Role access; unshared Skills stay isolated.
- [ ] Rollback records its result and does not restore broader script permissions or alter core code, Provider settings or Tool permissions.

**Goals coverage:** 25–26, 48–49.

## M4-06 — Run Flows and apply Disciplines safely

**Blocked by:** M4-01, M4-03.

**Observable delivery:** The owner deliberately starts an approved Flow and sees its fixed steps use relevant approved Disciplines through Job execution and Agora.

- [ ] A Flow can use Disciplines but attempted Flow-to-Flow invocation is rejected deterministically, including indirect references.
- [ ] Discipline selection cannot grant Tool authority, bypass approvals or exceed existing Step/Budget controls.
- [ ] Events/results identify the applied Skill versions and demonstrate that an ordinary task does not implicitly start a Flow.

**Goals coverage:** 10, 50, 61–62.

## Decisions required before readiness

Confirm Skill metadata, trust policy, review failures, script scope, sharing revocation and search/OAuth feasibility. Slice granularity and capability blockers are approved for publication. Issues are published needs-info pending owner policy decisions and prerequisite evidence. No new ADR is implied, and the approved compatibility spike remains a separate parent-managed feature.
