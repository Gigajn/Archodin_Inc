# M6 — Approved ticket publication

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M6-01 — Review and approve staffing changes

**Blocked by:** M1-02, M2-03, M2-04.

**Observable delivery:** Nakama presents a hire, departure or new Role proposal with workload/Budget evidence; the owner sees the effect of approval or denial in Agora and subsequent Jobs.

- [ ] All three change types require explicit content-bound owner approval; denial, stale approval or retry cannot create duplicate Agents/Roles or remove staff.
- [ ] A new Role declares instructions, Tools, approved Skills and Model chain without granting additional Tool/Provider authority; workload evidence alone changes nothing.
- [ ] Departure demonstrates the agreed assigned-work/access transition and records who decided, why and what changed.

**Goals coverage:** 21, 24, 51–53, supporting 35–37, 61–62.

## M6-02 — Approve a field-constrained Department Charter

**Blocked by:** M6-01.

**Observable delivery:** Explorador proposes a Department only within owner-allowed fields, optionally seeded from a pasted GitHub repository URL, and the owner approves or rejects its Charter.

- [ ] Charter review shows goal, work sources, Roles, Budget, repositories and requested external-action scope; unapproved or revised proposals cannot create a Department.
- [ ] Out-of-field discovery or repository suggestions cannot activate work; a linked repository is input, not permission to write or spend.
- [ ] Rejection and approval are auditable through headless events and Agora; configurable Departments do not require any particular business division.

**Goals coverage:** 2, 54–56, 61–62.

## M6-03 — Intake bounded GitHub issues behind human Jobs

**Blocked by:** M6-02, M2-05.

**Observable delivery:** An approved Department creates Jobs from eligible issues within owner-defined limits while human-origin Jobs receive scheduling priority.

- [ ] Scripted repeated polls and restart produce at most one active intake decision/Job per agreed source identity; rejected/ineligible issues show reasons.
- [ ] Scope, queue/concurrency quotas and Budget checks pause intake deterministically; issue text cannot enlarge Charter permissions.
- [ ] With human and issue Jobs queued, the human Job is selected first; tests document the agreed behavior for already-running automatic work and cancellation.

**Goals coverage:** 57–58, supporting 6, 35–37.

## M6-04 — Produce permission-bound, disclosed contribution PRs

**Blocked by:** M6-03, M1-03.

**Observable delivery:** An eligible issue Job produces a reviewed contribution through a fork, `<agent-name>/<topic>` branch and AI-disclosed PR, with its outcome visible in Agora.

- [ ] Scripted GitHub tests prove allowed-account/repository checks, target contribution-policy handling and refusal of direct upstream-main writes or insufficient permissions.
- [ ] PR output names the Agent and includes AI disclosure; prohibited or unclear project policies pause for an owner decision rather than silent submission.
- [ ] Retries, rate limits and restart after an ambiguous write reconcile persistent action identity before retry, avoiding duplicate forks/branches/PRs and leaking no credentials.

**Goals coverage:** 22, 59–60, supporting 5, 8–9.

## M6-05 — Verify auditable local operation and extraction readiness

**Blocked by:** M6-04, M4-06, M5-05, M3-06, M2-06.

**Observable delivery:** The owner receives an auditable mandatory-capability dashboard walkthrough and a local deployment/manual-extraction readiness report, without performing extraction or publication.

- [ ] Acceptance evidence connects staffing/Charter approvals, intake, contribution outcomes and token/cost accounting; authenticated remote access denies unauthenticated reads and controls and exposes no secrets.
- [ ] Bounded restart/resume, cancellation, duplicate-action recovery and backup/restore scenarios are exercised under owner-approved deployment policies; fake GitHub success is not presented as live integration certification.
- [ ] A reviewed asset/dependency/license inventory distinguishes MIT generic Archodin from private Basileia agents/prompts/workflows and Agora, records unresolved migration items, and leaves all remotes/repos untouched.

**Goals coverage:** 4–9, 37, 49, 61–62; confirmed product boundaries and local deployment scope.

## M6-06 — OPTIONAL: visualize the company as a 2D office

**Blocked by:** M6-05.

**Observable delivery:** If separately approved, an optional pixel-art office visualizes existing Agents, Departments and Job activity while preserving dashboard controls.

- [ ] Office activity derives from the same events and authorized controls; no separate scheduler, permission system or execution engine is introduced.
- [ ] Dashboard remains fully usable without the office; missing art/rendering support does not block mandatory release or operations.
- [ ] Owner separately approves visual scope and accessibility/performance acceptance; this ticket is not a blocker of any mandatory ticket or release.

**Goals coverage:** confirmed optional later 2D product target, supporting 4, 21.

## Decisions required before readiness

Confirm staffing thresholds/transition, allowed fields, Charter permissions, GitHub account and scopes, intake quotas/source identity, human priority versus preemption, repository policy handling and ambiguous-write reconciliation. Confirm audit retention, backup/restore objectives and authenticated remote deployment method; actual operational evidence is still required later. HR depends on identity, approval and Budget capability rather than completed Akasha learning. Published capability blockers remain prerequisites; keep optional office visual-scope approval separate. Legacy AAHC tickets must be revalidated rather than automatically promoted. No new technical framework or ADR is selected.
