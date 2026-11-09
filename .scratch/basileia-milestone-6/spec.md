# M6 — Approved organizational growth and bounded open-source operations

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

The owner wants Basileia to grow staff and Departments and contribute to selected open-source projects without surrendering control of scope, spending or external writes. Automatic issue intake can overwhelm human requests, duplicate work or violate repository policies. The owner also needs an auditable locally deployed system and a clear boundary for eventual manual repository extraction.

## Solution

Nakama proposes staffing and Role changes from workload and Budget evidence. Explorador searches only owner-allowed business fields and submits Department Charters, including proposals seeded by a GitHub repository URL. Agora presents concrete scope, responsibilities, costs and approval decisions. Approved open-source Departments may create bounded Jobs from repository issues, then contribute through forks, named Agent branches and disclosed pull requests. Human-submitted Jobs take scheduling priority.

Basileia remains a single-owner application on a workstation or mini-server with authenticated remote Agora access, not a multi-tenant service. The dashboard remains the primary operational surface. A later optional pixel-art office reuses existing events and controls; it neither replaces dashboard acceptance nor introduces another execution engine.

## User Stories

1. As the owner, I want evidence-backed hiring, departure and new Role proposals, so I control staffing and cost (Goals 51–53).
2. As the owner, I want allowed fields and reviewable Charters with goals, work sources, Roles and Budget, so growth follows my intent (Goals 54–55).
3. As the owner, I want to seed a Charter with a repository link, so I can choose projects worth contributing to (Goals 2, 56).
4. As the owner, I want bounded issue-to-Job intake that yields to my requests, so automation does not displace human work (Goals 57–58).
5. As a maintainer, I want fork-based PRs, identifiable Agent branches and AI disclosure, so project ownership and contribution policies are respected (Goals 22, 59–60).
6. As the owner, I want auditable outcomes and authenticated dashboard controls, so I can operate and recover the local company remotely (Goals 4–9, 37).
7. As a library user, I want generic headless execution and approval, so company organization remains outside Archodin (Goals 61–62).

## Implementation Decisions

The following safeguards are proposed. Nakama and Explorador have proposal authority only. Hiring, departures, new Roles and Charters require explicit owner approval bound to the proposed revision. Workload observations cannot create staff or Departments automatically. Charter scope includes allowed fields, target repositories/work sources, Roles, Budget and permitted external actions; edits beyond approved scope require another decision. Departures must resolve assigned work and access under an agreed transition policy.

Issue intake checks repository eligibility, approved Charter scope, concurrency/queue limits and remaining Budget before creating a Job. Human-origin work has deterministic scheduling priority; exact preemption behavior remains unresolved. A stable source-issue identity plus recorded intake decisions prevents duplicate Jobs across polling, retries and restarts. Untrusted issue text cannot expand permissions or override execution constraints.

External actions require approved repository/account scope and least necessary permissions. Contribute through a fork, an Agent-prefixed branch and a PR with AI disclosure; never directly push to upstream main. Respect target contribution rules and pause for uncertain permissions or AI restrictions. Persist action identity and outcomes; ambiguous failures require reconciliation before retry, avoiding duplicate forks, branches or PRs. Secrets are excluded from prompts, artifacts and audit displays.

Operational evidence links proposals, approvals, Jobs, costs and external writes. Deployment readiness includes restart/resume, authenticated remote access, cancellation and backup/restore checks under agreed policies. Extraction readiness inventories generic versus proprietary assets, dependencies and third-party licenses; it performs no split, remote creation, push or publication. No new framework, deployment topology or protocol is selected.

## Testing Decisions

Proposed seams remain the public headless Job/Graph API and approval interface, with Agora acceptance journeys. Scripted GitHub and Provider responses cover permissions, rate limits, duplicate issues, ambiguous writes and restart reconciliation without accounts or network. Verify staffing denial, stale approvals, field restrictions, human priority and Budget stops. Review deployment/extraction evidence separately from functional tests; do not infer production readiness from fake-service success alone.

## Out of Scope

Mandatory trading, media or customer-service implementations; autonomous organization changes; guaranteed free operation; SaaS billing; actual repository extraction or publication. The optional office does not gate required capabilities.

## Further Notes

Open decisions: workload thresholds and departure handover; allowed-field taxonomy; Charter permission granularity; GitHub credentials/scopes and repository-policy checks; intake quotas, priority/preemption and reconciliation; audit retention, backup targets and authenticated remote deployment method. Confirm optional office scope separately. Legacy AAHC tickets require revalidation; no new ADR is implied and the owner alone performs later repository separation.
