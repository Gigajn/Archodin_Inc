# Archodin — Provider and Tool compatibility spike

**Status:** ready-for-agent — the owner approved the two testing seams and five-ticket breakdown. Execute one eligible, unblocked ticket at a time; this does not authorize later milestones.

## Problem Statement

The owner needs evidence that Archodin can support Providers and typed Tools safely in Rust before committing to an integration dependency or building the full Graph. The existing AAHC service establishes a working Rust workspace but does not yet expose the proposed generic Provider/Tool contracts. Previous assumptions that langchain-rust had already been imported are not verified.

## Solution

Build a small, bounded compatibility spike exercised through generic public Provider and typed Tool interfaces. Use scripted offline adapters and evidence-based candidate comparisons to assess Tokio compatibility, typed errors, input/output contracts and cleanup. Return a reproducible recommendation, not a full agent runtime or an implicit dependency commitment. Basileia-owned agents and workflows remain outside Archodin.

## User Stories

1. As a library user, I want to submit a model request through a small generic Provider interface, so that my application does not depend directly on vendor SDK types.
2. As a library user, I want a normalized successful response, so that the calling code can use supported adapters consistently.
3. As a library user, I want unsupported capabilities reported explicitly, so that protocol compatibility is not mistaken for feature equivalence.
4. As a library user, I want failures represented as typed errors with actionable data, so that my application can decide what to do without parsing prose.
5. As a library user, I want authentication, rate-limit, transport and invalid-response failures distinguished where the adapter can determine them, so that later policy can classify them correctly.
6. As a library user, I want missing usage information represented as unavailable rather than zero, so that missing telemetry does not imply free work.
7. As a Tool author, I want structured input and output contracts, so that invalid input is rejected before execution.
8. As a library user, I want a Tool request validated and matched to an explicitly registered Tool, so that an arbitrary model-provided name cannot authorize execution.
9. As a library user, I want a valid Tool call and its result to remain correlated, so that an application can complete one bounded Provider/Tool round trip.
10. As a library user, I want invalid or unknown Tool requests to fail without execution, so that validation is observable and safe.
11. As a library user, I want asynchronous Provider and Tool operations compatible with Tokio, so that calling code can await them without a nested runtime.
12. As a library user, I want deadlines and cancellation propagated through owned work, so that a hanging adapter cannot keep the caller waiting indefinitely.
13. As a library user, I want concurrent independent calls to complete without leaked background tasks, so that parallelism does not compromise cleanup.
14. As a maintainer, I want behavioral tests with scripted adapters, so that correctness does not require internet access, credentials or inference spend.
15. As a maintainer, I want adapter mapping checks where an external protocol or SDK is used, so that a passing fake is not mistaken for proof of a real adapter.
16. As a maintainer, I want the same applicable contract scenarios used for competing candidates, so that the recommendation is based on comparable evidence.
17. As a maintainer, I want unknown or untested capabilities labeled honestly, so that documentation research is not presented as successful execution.
18. As the owner, I want requests, errors and reports to avoid credential disclosure, so that evidence artifacts are safe to review.
19. As a maintainer, I want the spike free of Basileia-specific agents, business error codes and employee persistence requirements, so that Archodin remains reusable.
20. As the owner, I want a concise recommendation with limitations and reproduction instructions, so that the next implementation phase starts from an artifact rather than this conversation.

## Implementation Decisions

- The owner approved this spike's testing seams and scope. No final dependency, crate layout or library interface signature is selected here.
- Keep one focused Provider contract and one typed Tool contract. Do not introduce a public coordinator solely to make unrelated contracts appear to have one test seam.
- For integrated evidence, use a bounded test scenario: request, response, optionally one registered Tool-call batch and its result. No recursive agent loop or full Graph is required.
- Reuse the existing Rust workspace, Tokio, structured data and typed-error patterns where appropriate. Add a dependency only when a candidate or test requirement justifies it; record its license and compatibility.
- Library-owned requests, responses and errors must not expose a vendor SDK as the public contract. Preserve supported capability distinctions; do not manufacture a universal feature set.
- Error categories should describe the new generic contract, not inherit legacy Python routing codes. Additional Budget, retry and fallback policies are not implemented by this spike.
- Inject transports/adapters and use deterministic responses for default tests. A local fake HTTP server may test wire mapping if needed; no real vendor, account, search or GitHub call belongs in the default suite.
- Dropping or cancelling a future must not be documented as terminating remote computation or forcibly stopping arbitrary blocking work. Demonstrate cleanup of work owned by the spike and record limitations explicitly.
- Assess at least two credible integration approaches, including langchain-rust if still viable. Apply the same bounded contract scenarios where executable; document a disqualification before implementation if evidence proves a candidate unsuitable. A direct adapter may be a candidate, not a preselected winner.
- Default evidence must not consume paid inference. No credentials are required by the test suite; usage and cost that cannot be known must remain explicitly unavailable.
- Preserve the legacy service and its tests. Do not migrate application behavior, change repository topology, or import Basileia agent assets into library code.

## Testing Decisions

- Proposed seams: the generic public Provider and typed Tool interfaces. Their contracts differ, so two small real seams are preferable to an artificial combined facade. Tests exercise behavior at those interfaces rather than private functions or SDK internals.
- Provider scenarios cover success, unavailable usage, unsupported capability, typed failure mapping and credential redaction.
- Tool scenarios cover valid structured input/output, correlation, invalid input and an unknown Tool without execution. A harmless scripted Tool is sufficient.
- Async scenarios cover a hanging operation reaching its deadline, caller cancellation, independent concurrent work and completion without owned background tasks leaking. Prefer controlled clocks, barriers and channels over fragile wall-clock sleeps.
- Adapter-specific fixture checks verify the selected external request/response mapping where applicable. Scripted fake success alone does not establish actual protocol compatibility.
- Follow TDD one behavior at a time: demonstrate a meaningful failing behavioral test, make the smallest implementation pass, then refactor. Do not bulk-write the complete suite first.
- Existing public-surface health tests and injectable configuration tests demonstrate offline behavioral testing and typed-error checks. Their HTTP/configuration contracts are not substitutes for Provider/Tool contracts.
- Each completed implementation ticket must pass the repository gate and record its output. The current passing legacy tests are a baseline, not evidence the new contract exists.

## Out of Scope

Full Graph, Department orchestration, Basileia agent definitions, Agora UI, persistent conversations, Skill discovery/installation, Budget reservations, model-chain policy, live model benchmarking, multi-tenant deployment, production credentials, repository split, publication, legacy feature migration and bulk ticket revalidation.

## Further Notes

- Approved Milestone 0 and product boundaries: `Goals.md`, `GLOSSARY.md`, `docs/adr/0001-archodin-basileia-boundary.md`, and `docs/adr/0002-repository-separation.md`.
- Tracking and execution eligibility: `docs/agents/issue-tracker.md`. Old AAHC frontier candidates remain subject to revalidation.
- Proposed ticket sequence and Rust Skill maintenance evaluation are in the sibling `ticket-proposal.md`; the owner-approved executable tickets are published individually in the feature's issues directory.
- Keep evidence concise: candidate, tested behavior, command/result, limitations and recommendation. Raw logs stay in artifacts and are referenced rather than repeatedly copied into agent context.
- The Rust Skill currently includes legacy Python error-code and employee rules. Until scoped maintenance is approved, apply the accepted generic-core boundary rather than interpreting those application rules as Archodin requirements.
