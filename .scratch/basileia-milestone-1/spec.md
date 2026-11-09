# Milestone 1 — One bounded software Job end to end

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Problem Statement

The owner needs a working demonstration of Basileia completing a small software Job, rather than isolated runtime scaffolding. Archodin must remain independently usable and free of proprietary company behavior. Existing service/configuration/database code may help, but its legacy names and contracts cannot be assumed to be the final library design.

## Solution

After a positive compatibility-spike recommendation is accepted, deliver a minimal headless Job run and a basic Agora route to submit and inspect it. Add persistent named Agent/Role configuration, controlled project inputs, isolated software-work Tools, local results and honest usage accounting. Keep the Graph deliberately small; advanced composition comes later.

## User Stories

1. As the owner, I want to submit one bounded Job, so that I can observe a useful completed task.
2. As a library user, I want the same basic run available without Agora, so that my application does not depend on Basileia.
3. As a library user, I want a minimal typed-State Graph with deterministic steps and a Step limit, so that execution is bounded from the beginning.
4. As the owner, I want named Agents created from Role instructions and allowed Tools, so that results identify their responsible worker.
5. As the owner, I want configuration retained between runs/restarts, so that an Agent is an identity rather than a permanent idle task.
6. As the owner, I want an explicitly configured Provider account/model used, so that paid or incompatible routes are never selected implicitly.
7. As the owner, I want to attach files, links and repository references within limits, so that Jobs have usable source material without granting arbitrary access.
8. As the owner, I want software changes and tests confined to a designated isolated project environment, so that generated code cannot run freely on the host.
9. As the owner, I want local changes, test outcomes and a summary, so that I can inspect a completed Job without an automatic push or deployment.
10. As the owner, I want Agora to show run events, artifacts and terminal status, so that I can follow success or failure.
11. As the owner, I want token/usage and cost availability made explicit, so that an unknown amount is not presented as zero.
12. As the owner, I want authenticated access and a safe local default, so that administrative controls are not unintentionally public.

## Implementation Decisions

- Proposed: accept the spike's evidence and unresolved limits before selecting a dependency or local crate arrangement. A no-go recommendation requires a decision, not automatic implementation.
- Archodin owns reusable execution primitives; Basileia owns company configuration, persistence policy, instructions and Agora. Prove a headless consumer can use the library without proprietary assets.
- Start with a minimal linear Graph and in-memory checkpoints/events. Persistent crash recovery is a Milestone 2 capability, not claimed here. Keep a Step limit and deadlines even in the first demonstration.
- Agent records can persist while execution sessions are bounded and separate. Do not spawn an idle process or loop per employee.
- Use the selected Provider adapter with an explicit base URL/account/model and capability checks. Default acceptance uses fakes. Any live test is opt-in and requires owner-approved credentials and spend limits.
- Tools begin with harmless fixtures; generated code execution is enabled only after isolated-workspace enforcement and escape checks are implemented. Reject out-of-scope paths, unsafe attachment access and unexpected network/host authority.
- Agora is a small operational dashboard using the same run interface. Keep initial remote access disabled until authenticated transport is configured and tested. No 2D renderer or frontend framework is selected by this planning baseline.
- Request/step/concurrency limits and explicit spend permission bound this demonstration. Do not advertise unattended operation or atomic monetary hard caps before Milestone 2.

## Testing Decisions

- Proposed primary seam: submit a Job through the public headless run interface and assert emitted events, terminal outcome and artifacts. Provider/Tool contract tests from the spike remain adapter evidence.
- Agora has an additional acceptance surface for submission, authenticated inspection and matching run status; do not assert private database or UI component layout.
- Scripted Providers and an isolated fixture workspace make the standard demonstration deterministic and offline. Assert local changes/test results plus denied escapes, invalid inputs, no unsolicited external actions and unavailable telemetry.
- Existing health/configuration tests are regression evidence only. New behavior follows one red→green cycle at a time; repository gate evidence belongs in each completed ticket.

## Out of Scope

Full parallel/nested Graph, Strategos planning, model-chain fallback policy, durable restart/resume, Skill discovery, Akasha, autonomous business Departments, automatic push/deploy/financial transactions and 2D office. Repository extraction/publication remains owner-managed.

## Further Notes

This is approved planning scope derived from Goals 1–2, 4–5, 7, 21, 23–24, 29, 32, 37 and 61, plus the confirmed first-demonstration target. Sources: `Goals.md`, `GLOSSARY.md`, both accepted ADRs and the compatibility-spike spec. See sibling `ticket-proposal.md`. Isolation implementation, authentication method, attachment/network policy and exact Job/Graph interface require confirmation before affected tickets become ready.
