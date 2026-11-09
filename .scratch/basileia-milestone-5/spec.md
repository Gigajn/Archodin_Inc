# M5 — Akasha knowledge and experience-driven Skill proposals

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

Long-lived Agents should improve across Jobs without sharing entire private conversations or accumulating unbounded context. A Role needs reliable facts about its projects, clients and decisions, but duplicate, stale or contradictory notes can increase token costs and mislead later work. Successful runs and owner corrections should also yield reusable Skill improvements without quietly authorizing new instructions or execution.

## Solution

Provide a Role-owned Akasha with project-aware retrieval, source provenance and a visible curation trail. Codex proposes concise, deduplicated knowledge from eligible Job material; the owner can inspect and correct it in Agora. Agents retrieve relevant bounded excerpts rather than receiving every colleague's session history. Approved Skills and Akasha are the intended automatic-maintenance surfaces, not core code, Provider settings or Tool permissions.

Experience can propose a new Skill or refinement after a successful run with at least five Tool calls, or after an owner correction. This preserves the requested trigger as a proposal for confirmation, not a replacement threshold invented by the roadmap. Every resulting Skill goes through the existing acquisition/review/pending approval lifecycle, including separate script approval. Improvement is visible and reversible rather than self-authorizing.

## User Stories

1. As the owner, I want named, long-lived Agents to retain useful learning between Jobs, so the company improves without merging their identities (Goals 21, 23).
2. As an Agent, I want my Role's relevant project knowledge with evidence, so I can reuse facts responsibly (Goals 27).
3. As the owner, I want Codex to condense and deduplicate Akasha entries, so retrieval stays small and inexpensive (Goals 28).
4. As an Agent, I want my ongoing conversation private, so colleagues' unfinished work does not fill my context (Goals 29).
5. As the owner, I want success and corrections to propose Skills, so experience becomes reviewed reusable capability (Goals 42–48).
6. As the owner, I want controlled knowledge maintenance, so learning cannot reconfigure runtime authority (Goals 49).
7. As a library user, I want learning exercised through headless Jobs and approval interfaces, so knowledge behavior is not coupled to Agora (Goals 61–62).

## Implementation Decisions

All decisions in this section are proposed. Akasha entries carry Role scope, project scope where applicable, source Job/material reference, authorship, revision history and freshness/curation state. Retrieval enforces scope before ranking or summarization; untrusted stored text is evidence, never runtime instructions. Cross-project reuse requires an explicit allowed scope rather than a search similarity match. Private sessions remain private unless material is deliberately promoted under the agreed policy; retrieval cannot traverse another Agent's conversation by default.

Provide bounded retrieval with inspectable citations, selected excerpts and truncation/absence signals. Preserve sources through condensation and deduplication. Conflicting claims remain identifiable instead of silently selecting the latest completion. Mark superseded or stale entries and allow correction, withdrawal and re-curation; avoid claiming facts are fresh merely because they were retrieved. Do not select a database, embedding service, vector framework or retrieval protocol here.

A learning proposal records the qualifying run or correction, proposed changes and rationale. Count actual eligible Tool invocations under a policy still to be confirmed; the proposed five-call trigger must not encourage extra calls merely to qualify. Skill proposals cannot bypass Vigil's static-plus-LLM review, Codex's pending queue, content-bound owner installation approval, version history or separate script authorization. Repeated corrections or retries should not flood the queue with identical proposals.

Codex's automatic Akasha condensation must remain within agreed scope, retain evidence and support inspection and reversal. Exact promotion, deletion and retention policies require owner approval. Knowledge and Skill maintenance cannot mutate Provider configuration, core code or Tool permissions; runtime enforcement remains deterministic even when Agents propose those changes.

## Testing Decisions

Use the proposed public headless Job/Graph API and approval interface, with Agora acceptance checks of citations, curation history and pending proposals. Scripted Providers and local knowledge fixtures cover Role/project isolation, private-session exclusion, duplicates, stale/conflicting sources and bounded retrieval without external accounts. Demonstrate the five-call and correction paths with deterministic Tool outcomes, then review/reject/approve through the same Skill lifecycle. Assert observable results and permission boundaries, not storage layouts or exact LLM wording.

## Out of Scope

A globally readable memory, automatic promotion of all conversations, autonomous Skill installation, unlimited context, or selecting new infrastructure. This milestone does not implement trading, media or customer-service businesses or promise zero operating costs.

## Further Notes

Open decisions include Role-wide versus project-specific facts; promotion consent; sensitive-data retention and deletion; staleness thresholds; conflict handling; retrieval limits; eligible call counting, repeated-run suppression and trigger opt-out. No policy is confirmed merely by inclusion here. Existing product boundaries and manual future repository separation remain intact; legacy AAHC work still needs revalidation.
