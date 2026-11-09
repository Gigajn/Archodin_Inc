# Milestone 0 — Align Archodin and Basileia sources of truth

## Purpose

Reconcile the existing AAHC baseline and Goals before selecting a runtime integration or resuming legacy implementation. This milestone produces documentation and execution policy, not an implemented agent runtime.

## Authoritative references

- Product vision, confirmed scope and remaining proposals: `Goals.md`.
- Product terminology: `GLOSSARY.md`.
- Core/application ownership and licensing: `docs/adr/0001-archodin-basileia-boundary.md`.
- Local development and owner-managed future repository separation: `docs/adr/0002-repository-separation.md`.
- Document ownership and conflict handling: `docs/agents/domain.md`.
- Work tracking, implementation eligibility and AAHC revalidation: `docs/agents/issue-tracker.md`.
- Preserved historical baseline and explicit reconciliation notice: `.scratch/aahc/spec.md`.

## Acceptance criteria

- Archodin, Basileia and Agora have distinct canonical meanings.
- Archodin is an independent MIT-licensed library; Basileia's company-specific agents, instructions and workflows remain application-owned and closed-source.
- Rust is preferred without prematurely selecting an integration dependency or UI framework.
- Future repositories are separate; development remains local and the owner performs the final separation and pushes.
- langchain-rust is a candidate, not an assumed imported dependency or mandatory fork. A focused Provider/Tool/async/error/offline-testing spike precedes the full Graph.
- Goals distinguish confirmed product scope from implementation proposals and remove unsupported upstream/test claims.
- Initial deployment, first operational demonstration, optional Departments, dashboard-first UI and bounded-cost policy match the owner's confirmed choices in Goals.
- Legacy tickets remain intact and require owner-confirmed revalidation before implementation. The frontier script's filtering limitation is documented without claiming it has been fixed.
- Documentation checks and `./scripts/check.sh` results are recorded in the milestone ticket before it can be marked done.
- The owner confirms the final shared understanding before the milestone is closed.

## Verification

Check document references, whitespace and the separation between generic library mechanisms and Basileia-specific definitions. Run the repository gate; do not change unrelated code to force a pass. This documentation-only milestone does not justify new runtime tests or claims of end-to-end behavior.

## Out of scope

Runtime implementation, dependency selection, live Provider calls, source-file migration, legacy ticket execution or bulk status changes, repository separation, remote configuration and publication. Exact crate ownership/license metadata and detailed runtime semantics are follow-up work, not silently resolved here.

## Next work, not yet started

Draft a focused compatibility spike with public-boundary offline tests and an evidence-based dependency recommendation. Separately plan legacy-ticket revalidation and a test-first frontier eligibility correction. Do not treat these descriptions as ready implementation tickets.
