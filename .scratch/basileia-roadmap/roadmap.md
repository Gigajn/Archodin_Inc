# Basileia and Archodin — Complete remaining roadmap

## Status and reading order

**Milestone 0 is done.** Its decisions are the product baseline, not completed runtime code.

**Compatibility spike: approved and published.** The five same-feature issues may be executed in dependency order, one eligible ticket at a time. Nothing in this roadmap starts that implementation automatically.

**Milestones 1–6 and support work: approved planning baseline.** The owner accepted the full roadmap and testing seams. The 35 milestone slices are published as individual `needs-info` issues, with qualified blockers and explicit ticket-local readiness conditions; optional office work still needs its separate visual-scope decision. Two support issues are published separately. No policy is resolved merely by changing a planning document's status.

Start with this index; load the relevant spec/proposal only when planning or implementing that capability. Use concise artifacts at phase boundaries rather than replaying this whole conversation.

## Milestones and exit evidence

| Milestone | Deliverable | Exit evidence | Documents |
|---|---|---|---|
| M0 — Sources of truth | Accepted names, ownership, license intent and work policy | Owner confirmation and repository gate already recorded | `../basileia-milestone-0/spec.md` |
| M1 prerequisite — Compatibility spike | Evidence for Provider, typed Tool, Tokio/error/offline compatibility; scoped Rust Skill maintenance | Owner-reviewed Skill evaluation and accepted positive integration recommendation, or explicit no-go | `../archodin-compatibility-spike/spec.md`, approved `issues/01`–`05` |
| M1 — First complete Job | Named Agent, bounded minimal Graph, isolated local software change/tests, inputs, usage and basic Agora | Repeatable local Job with events, patch/test artifacts and honest costs; no automatic external write | `../basileia-milestone-1/spec.md`, `ticket-proposal.md` |
| M2 — Controlled execution | Durable recovery, cancellation, generic approval, atomic Budget accounting, configured model chains, secure control | Offline failure/concurrency/restart/denial tests and authenticated operational acceptance | `../basileia-milestone-2/spec.md`, `ticket-proposal.md` |
| M3 — Delegation and context | Strategos, Briefing, bounded history, conditional/parallel/nested Graphs and Panel/Qadi | Fresh-context nested run with deterministic invariants, traceable semantic decisions and child accounting | `../basileia-milestone-3/spec.md`, `ticket-proposal.md` |
| M4 — Curated Skills | Ronin/Vigil/Codex acquisition/review, exact-content approval, script separation, sharing/rollback and Flows/Disciplines | Local/external fake-source rejection/approval and role/version/script safety evidence | `../basileia-milestone-4/spec.md`, `ticket-proposal.md` |
| M5 — Learning and Akasha | Scoped cited knowledge, curation/freshness, private sessions and experience-derived pending Skills | Two-Job reuse with no cross-scope/session leakage and the same Skill approval lifecycle | `../basileia-milestone-5/spec.md`, `ticket-proposal.md` |
| M6 — Bounded organization and delivery readiness | Nakama/Explorador approvals, Charters, prioritized issue intake, compliant disclosed PR workflow, deployment/extraction readiness | Integrated mandatory-capability walkthrough, recovery/security checks and asset/license inventory | `../basileia-milestone-6/spec.md`, `ticket-proposal.md` |
| Later optional target | 2D office visualization using existing events/controls | Separately approved visualization/accessibility/performance acceptance; never gates mandatory release | M6-06 proposal |

The table is an implementation narrative, not a coarse dependency chain. Concrete blockers live in each proposal. Some M4/M5/M6 slices can follow their proven prerequisite capabilities without waiting for an unrelated complete milestone.

## Proposed slice inventory

- Supporting: **P-01** eligible/qualified frontier; **P-02** legacy evidence/revalidation.
- Approved spike: **S-01** scoped Rust Skill; **S-02** Provider interaction; **S-03** Tool round trip; **S-04** async bounds/cleanup; **S-05** integration recommendation. `S-NN` is the roadmap alias of compatibility-spike issue NN, not an extra ticket.
- M1: **M1-01** headless Job; **02** named identity/Role; **03** isolated software result; **04** Agora submission; **05** attachments/usage; **06** first demonstration.
- M2: **M2-01** durable recovery; **02** cancel/limits; **03** approval; **04** Budget; **05** model chains; **06** authenticated operational acceptance.
- M3: **M3-01** branching/functions; **02** Briefing delegation; **03** parallel/subgraph merges; **04** Panel/Qadi; **05** history/context; **06** nested demonstration.
- M4: **M4-01** local matching; **02** external provenance; **03** review/install approval; **04** script approval; **05** version/share/rollback; **06** Flows/Disciplines.
- M5: **M5-01** scoped retrieval; **02** curation; **03** staleness/withdrawal; **04** experience proposals; **05** learning reuse.
- M6: **M6-01** staffing; **02** Charters; **03** issue intake; **04** contribution PRs; **05** integrated readiness; **06 optional** office.

**Counts:** 5 published approved spike tickets, 34 published mandatory milestone issues, 1 published optional milestone issue and 2 published support issues. The 35 later milestone issues remain needs-info; the current frontier, not this count, determines implementation eligibility. A proposed slice can be split if measured complexity requires it, with approval rather than silent scope expansion.

## Coverage of all current Goals user stories

Numbers refer to the stable 63-story list in `Goals.md`; assigning a story is a plan, not implementation evidence.

| Goal story IDs | Primary milestone/capability | Completion notes |
|---|---|---|
| 1, 2, 4, 5 | M1 | Browser submission, bounded input, progress and result; later milestones extend visibility. |
| 3 | M3 | Strategos planning/delegation belongs to Basileia. |
| 6, 7, 8, 9 | M2 | Cancellation, defined Step limits, durable resume and approval; minimal M1 limits are not full recovery. |
| 10 | M4 | Explicit Flow execution integrates with M3 delegation; Flow never invokes another Flow. |
| 11, 12, 13, 14, 15, 16, 17, 18 | M3 | Nested organization/Graphs, pure routing, typed State, parallel merges and application Qadi. |
| 19, 20 | M3 | Five-section Briefing and authorized bounded history retrieval. |
| 21 | M1 | Persistent named Agent and Role shown in run results/Agora. |
| 22 | M6 | Agent-prefixed branches in future contribution workflow. |
| 23 | M1 and M5 | Identity persistence first; curated cross-Job learning later. |
| 24 | M1, M2 and M4 | Role instructions/Tools first; model chains and approved Skills complete it. |
| 25, 26 | M4 | Approved same-Role access and controlled cross-Role sharing. |
| 27, 28 | M5 | Scoped cited Akasha, bounded and deduplicated. |
| 29 | M1, M3 and M5 | Private sessions, context isolation and controlled promotion to knowledge. |
| 30, 31, 32, 33 | M2 | Explicit accounts/chains/capability-safe fallback; compatibility and initial configurable adapter begin in spike/M1. |
| 34 | M3 | Quality handled by Panel/Qadi, not technical fallback. |
| 35, 36, 37 | M2 | Agent/Job Budget pause/approval and full attributable accounting; basic M1 telemetry is not a hard cap. |
| 38, 39, 40, 41 | M4 | Local-first discovery, configured search/GitHub, optional login and source trust. No paid search requirement is not a promise of zero infrastructure/inference cost. |
| 42 | M5 | Successful-run/five-Tool-call or correction-triggered Skill proposals; detailed counting/dedup policy remains proposed. |
| 43, 44, 45, 46, 47, 48 | M4 | Review, reasons, exact-version/group installation approval, separate scripts and rollback. |
| 49 | M4, M5 and M6 acceptance | Learning authority limited to approved surfaces; never core-code/Provider/permission self-modification. |
| 50 | M4 | Flows and Disciplines; runtime controls enforce invocation constraints. |
| 51, 52, 53 | M6 | Nakama proposes staffing/Roles; owner approves. |
| 54, 55, 56 | M6 | Allowed fields, Charters and repository-seeded proposals. |
| 57, 58 | M6 | Bounded issue intake and human-Job priority. |
| 59, 60 | M6 | Fork/Agent branch/disclosed PR, with permission and project-policy checks. |
| 61, 62 | All required milestones | Headless generic core and frontend-independent approval; never company agents in the public library. |
| 63 | Compatibility prerequisite | Evidence-based integration choice, including a possible no-go; no mandatory langchain-rust fork. |

## Test seam approval and readiness

The two Provider/Tool seams and five spike tickets are already approved. For the remaining roadmap, proposed main seam is public Job/Graph execution/control with events and final State/artifacts; approval and Agora acceptance remain real additional surfaces. This is not a proposal to test private implementation or introduce a fake universal facade.

Roadmap/slice/seam approval is recorded. Later issues are published with explicit cross-feature blockers and needs-info status. Before promoting an issue to ready-for-agent, resolve its listed policy choices and readiness evidence; only then may the frontier offer it after all prerequisites are done. P-01 supplies the tested eligibility guard. Open AAHC tickets still require P-02/relevant owner-confirmed revalidation.

## What 'complete' means

All mandatory approved capabilities have passed their own gates and the integrated M6 acceptance, supported by reproducible evidence and documented limits. This does not mean arbitrary future business Departments are implemented, optional office visualization is required, every vendor is feature-identical, or live service certification follows from fakes. The owner performs final repository separation and pushes manually; future Basileia contribution features do not authorize this coding agent to push the current repository.
