# Proposed tickets — compatibility spike and focused Skill maintenance

**Status:** approved — the owner accepted the two public testing seams and five-ticket breakdown. Individual tickets are now published; no runtime or Skill changes have started.

This proposal uses independently verifiable slices. It does not require schema, HTTP or UI changes for a headless library spike. The completed Milestone 0 is a prerequisite already satisfied, not an open blocking ticket.

## 01 — Scope Rust conventions without losing legacy guidance

**Blocked by:** None.

**Delivers:** Rust guidance that clearly separates generic Archodin rules from Basileia/legacy application constraints. Preserve legacy error semantics where they still apply, but prevent them from becoming library requirements. Clarify that examples of crates do not authorize new dependencies. Keep the existing Skill name and separate responsibilities; do not merge TDD, clean-code and token-efficiency into one large Skill.

**Verification:** Preserve a baseline snapshot before editing. With skill-creator, compare baseline and updated guidance on the same approved evaluation prompts. Check scope correctness, retained Tokio/offline/error discipline and lack of unauthorized dependency or application commitments. Report model evaluation results honestly; static content checks alone are not behavioral benchmarks. Capture available usage/time rather than inventing unavailable metrics. Produce a static evaluation viewer for owner review before accepting the update.

**Proposed evaluation prompts:**

1. Design a generic Provider contract for an MIT Rust library with no selected integration dependency. Identify errors and testing. Expected: no employee model or mandatory legacy routing codes; no unjustified dependency commitment.
2. Plan a typed Tool with a scripted fake and a hanging asynchronous call. Expected: input validation, typed errors, Tokio-compatible deadlines/cancellation, and offline behavioral tests through public interfaces.
3. Maintain the legacy application's Python-parity router. Expected: preserve its established error semantics within that legacy context rather than deleting or generalizing them away.

## 02 — Complete one offline Provider interaction

**Blocked by:** 01.

**Delivers:** A caller can submit a request through the small generic Provider contract and observe either a normalized response or a typed failure, using a scripted offline adapter. Usage may be unavailable; errors and diagnostics do not reveal credentials. This is a complete headless interaction, not an empty trait scaffold.

**Verification:** Public-interface tests demonstrate success and failures one behavior at a time; no internet or credentials required. Existing tests and repository gate remain green.

## 03 — Complete one validated typed Tool round trip

**Blocked by:** 02.

**Delivers:** A bounded Provider interaction can request an explicitly registered harmless Tool; validated structured input produces a correlated structured result. Invalid input and unknown Tool names do not execute a Tool. No general agent loop or arbitrary script execution is introduced.

**Verification:** Observe successful round-trip output and no-execution failure cases through the public contracts, not internal registry layout. Record red→green evidence and the gate.

## 04 — Bound and clean up asynchronous interactions

**Blocked by:** 03.

**Delivers:** Hanging Provider/Tool operations reach a deadline, caller cancellation propagates, and independent operations can overlap without leaking work owned by the spike. The report distinguishes cancelling local work from stopping remote or arbitrary blocking work.

**Verification:** Deterministic Tokio tests use controlled timing/synchronization; demonstrate typed outcomes and owned task cleanup. No detached-task or sleep-based timing assumptions. Record red→green evidence and the gate.

## 05 — Compare integration candidates and publish the recommendation

**Blocked by:** 04.

**Delivers:** An evidence-backed choice or an explicit no-go, using at least two credible approaches. Apply the same applicable request/Tool/error/async scenarios to executable candidate adapters; document supported behavior, limits, dependency/license implications and reproducible commands. Candidates may be disqualified with evidence rather than implemented needlessly. Keep the generic library contract independent of the winning SDK.

**Verification:** Protocol/SDK mapping is exercised through fixtures or an injected transport, not inferred from a scripted generic fake. Separate executed checks from documentation findings and unknowns. Default checks need no vendor API or inference spend. The report is the handoff to later Graph work; it does not itself authorize that work or publish a repository.

## Approval recorded

The owner confirmed the two public testing seams, five-ticket granularity and blocking edges. The candidate-comparison slice may be split after research only if concrete candidate complexity makes it exceed one fresh implementation context.
