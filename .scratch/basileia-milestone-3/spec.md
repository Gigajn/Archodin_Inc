# Milestone 3 — Delegation, graph composition and bounded context

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

A useful multi-agent company needs decomposition and parallel work without copying every conversation into every prompt, losing concurrent results or replacing runtime invariants with an LLM judgment. The minimal Graph and execution controls must become reusable composition primitives while Basileia supplies its own Strategos and Qadi behavior.

## Solution

Add conditional/function/subgraph composition, explicit State merge behavior, structured Handoffs and bounded history retrieval. Demonstrate Basileia's Strategos delegating through fresh execution sessions and its Panel/Qadi pattern comparing semantic results. Maintain accounting, approvals and recovery across child work.

## User Stories

1. As a Graph author, I want Agent/function/subgraph Nodes, so that computation does not require an LLM when code is enough.
2. As a Graph author, I want conditional routing from typed State, so that execution follows explicit decisions.
3. As a Graph author, I want parallel independent Nodes, so that work can overlap safely.
4. As a Graph author, I want defined field merge rules, so that timing does not silently discard results.
5. As the owner, I want Strategos to propose and assign bounded subtasks, so that I need not manually decompose every Job.
6. As an Agent, I want a Handoff Briefing with Goal, Done so far, Decisions, Open questions and Related material, so that the receiver starts with essentials.
7. As a receiving Agent, I want bounded authorized history lookup, so that short Briefings remain traceable without automatic full-history replay.
8. As the owner, I want private sessions separated from persistent identities, so that long-lived Agents do not accumulate unbounded prompts.
9. As the owner, I want nested Strategoi/Departments, so that organization can compose without multiplying context or resetting limits.
10. As the owner, I want a Panel and Qadi to assess competing semantic answers, so that poor quality does not masquerade as technical fallback.
11. As the owner, I want runtime invariants protected from Qadi decisions, so that money, permissions and approval are never won by argument.
12. As the owner, I want child work visible in Agora and charged to its Job, so that delegation does not hide progress or cost.

## Implementation Decisions

- Proposed: expose generic Graph/run composition in Archodin. Strategos, Qadi, their Role instructions, planning prompts and Department policies stay in Basileia; Panel/Handoff are compositions, not built-in company agents.
- Separate authoritative stored State from a Node's prompt projection. Store artifact references and expose only needed information to each call; shared State is not a shared full transcript.
- Define deterministic append/overwrite/conflict semantics and ordering. Invoke an application-supplied semantic resolver only for eligible competing content, never permission/Budget/approval invariants.
- Bound nested depth, fan-out, Step counting and retries under the parent Job's allowances; propagation and checkpoint boundaries must cover child work.
- Briefings carry the requested five sections, acceptance constraints, references and verification/uncertainty when relevant. A fresh-context receiver must continue without hidden conversational assumptions.
- Read history through scoped, paginated, bounded Tools; an Agent's private ongoing work does not automatically become peer context.
- Context policy includes model-specific input/output headroom, tool-result limits, phase artifacts and fallback capability checks. The token-efficiency 40% heuristic is a tunable starting point, not a quality guarantee or fixed token count.
- Role/Tool/Skill context loads on demand when supported, with stable serialization where useful. Provider-specific caching optimizations are optional and measured; do not promise cache reuse across models/accounts.

## Testing Decisions

- Proposed primary seam: submit/inspect a Graph or Job through public execution interfaces and assert final State, routing and events. Inject scripted planning/judgment Providers, action adapters, clocks and approval decisions.
- Test pure routing without an LLM, concurrent append/conflict outcomes independent of finish order, child Step/Budget/cancel propagation, nested recovery and refusal of invalid composition.
- Run a fresh-context Handoff scenario and assert essential references/constraints survive while full history is absent. Test permitted retrieval, denied scope and output limits.
- Panel/Qadi fixtures may select or combine semantic outputs but cannot grant Tools, alter accounting or approve an action. Record all supporting usage and failure outcomes.
- Agora acceptance inspects the same child run tree and decisions. Test behaviors, not prompt wording or private structures; retain TDD and per-ticket gate evidence.

## Out of Scope

Autonomous hiring, external Skill acquisition, shared Akasha learning, financial trading and optional office rendering. This milestone does not guarantee that more agents or shorter summaries improve task quality without evaluation.

## Further Notes

Covers Goals 3, 11–20, 29, 34 and 61, with Flow integration continued in M4 and operational controls inherited from M2. Sources: `Goals.md` and the accepted product-boundary ADR. See sibling `ticket-proposal.md`. Merge order, eligible semantic conflicts, nested Step semantics, context thresholds and Briefing validation need explicit decisions before affected tickets are ready.
