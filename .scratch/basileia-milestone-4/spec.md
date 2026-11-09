# M4 — Curated Skills and controlled capability acquisition

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

Basileia needs to acquire reusable know-how without turning downloaded instructions into execution authority. The owner needs visibility into sources, review findings, costs and installed versions. Agents of one Role should learn together without unrelated Roles receiving every Skill. Discovery must remain useful with local material and without paid search or mandatory external login.

## Solution

Build on proven Agent identity and approval controls to offer an Agora dashboard journey from a Job's missing capability to a reviewed, owner-approved Skill. Ronin proposes matches, Vigil produces static and LLM assessments, and Codex manages pending candidates, installation and versions. These are Basileia Roles and workflows, not built-in Archodin agents. Generic Skill and approval mechanisms remain usable headlessly.

Search local approved Skills first. When acquisition is needed, use configured self-hosted SearXNG, otherwise a DuckDuckGo integration, plus GitHub discovery; optional GitHub login enables capabilities such as higher limits and code search where supported. Free discovery means no paid search service is required, not zero hosting, network or LLM cost. Unavailable routes and exhausted Budgets produce visible waiting or failure, never an unapproved paid route.

## User Stories

1. As the owner, I want Ronin to identify missing Skills before assignment, so Agents begin with suitable knowledge (Goals 38–40).
2. As the owner, I want source trust, provenance and Vigil findings visible, so I can assess unfamiliar repositories (Goals 41, 43–44).
3. As the owner, I want pending approvals grouped by Role, including explicit group approval, so installation remains my decision (Goals 45–46).
4. As the owner, I want scripts separately approved and versions reversible, so instruction approval cannot silently execute code (Goals 47–48).
5. As an Agent, I want approved Role Skills and explicitly shared Skills, so learning is reusable without context flooding (Goals 24–26).
6. As a workflow author, I want explicit Flows and task-applicable Disciplines, so structured work stays understandable (Goals 10, 50).
7. As a library user, I want headless Skill decisions and approvals, so Agora is not mandatory (Goals 61–62).

## Implementation Decisions

All runtime decisions here are proposed. Store Skills in the requested SKILL.md format, with kind, Role scope, source reference, acquired revision or content identity, trust classification, review evidence and installation version. External content is untrusted data; a trusted source still requires review and approval. Preserve applicable license and notice information; unresolved permission to reuse blocks installation pending owner resolution.

Vigil runs deterministic static checks before LLM review. Suspicious instructions, script contents and provenance are included in findings; neither reviewer can grant permissions. Failed or unavailable review cannot become implicit approval. Codex binds approval to the reviewed candidate content and intended Role scope; edits invalidate that decision. Group approval displays exact candidates and findings. Runtime checks, not agent promises, enforce approval and Budget boundaries.

Installation does not enable or run scripts. Separate script approval is bound to script content and execution scope and remains subject to existing execution isolation and Tool permissions. Rollback restores the selected installed version without inheriting broader script authority. Role colleagues see approved updates; cross-Role sharing requires an explicit Codex shared designation within owner-approved scope. Automatic maintenance cannot change core code, Provider configuration or Tool permissions.

Flows are deliberately invoked fixed-step procedures, may use Disciplines, and cannot invoke another Flow. Disciplines apply when relevant; their text cannot override runtime limits. No search implementation, framework or new protocol is selected here.

## Testing Decisions

Proposed primary seam: public headless Job/Graph execution and approval interface, with Agora acceptance checks for the same events and decisions. Scripted search, GitHub and Provider responses require no accounts or network. Demonstrate local hits, unverified acquisition, review rejection, stale approvals, blocked scripts, Role sharing and rollback. Assert observable outcomes, not internal agent messages; do not invent a universal facade or claim unverified test precedent.

## Out of Scope

Autonomous installation, permission escalation, recursive Flows, company agents in core, paid-search requirements, and implementation of business divisions. Owner approval authorizes issue publication only; this document authorizes no implementation of needs-info work or repository separation.

## Further Notes

Open decisions: supported Skill metadata compatibility; trust-list ownership and strict-review rules; review retry policy; script approval granularity; shared-scope revocation and rollback semantics; search adapter feasibility and OAuth permissions. The owner must approve these details. M0's MIT library/private application boundary and manual future repository extraction remain unchanged; legacy AAHC tickets require revalidation.
