# M1 — Approved vertical slices

**Status:** APPROVED PLANNING BASELINE — owner approved the complete roadmap, specs/seams and slice breakdown, then confirmed “tdd okey”. Tickets are now published as needs-info until the listed local policies and prerequisites are resolved; this is not implementation authorization for needs-info work.

## Publication and readiness

All slices below are published as numbered issues in this milestone feature, preserving their stable Mx-NN identities. Every issue has basileia-roadmap#01 as a publication/eligibility blocker, in addition to its actual capability blockers. The P-01 guard is implemented and active: qualified feature#NN dependencies, fail-closed eligibility and recorded-approval checks for legacy work are enforced. Current implementation status and gate evidence are owned by support issue basileia-roadmap#01. Owner policy decisions and prerequisite evidence must be recorded before an affected issue can be promoted to ready-for-agent. This publication does not block or change the separately approved compatibility spike.

## M1-01 — Complete a minimal headless Job

**Blocked by:** S-05 and owner acceptance of a positive spike outcome.

**Delivers:** A bounded linear Graph accepts typed input, executes a scripted step and returns final State and run events to a headless caller.

- [ ] Success, typed failure and the Step limit are observable without Agora or proprietary agents.
- [ ] In-memory checkpoints reflect completed steps; restart recovery is not claimed.
- [ ] Local library ownership/dependencies are inspectable and contain no Basileia agent assets; regression gate and incremental tests pass.

## M1-02 — Run with a persistent named Agent and Role

**Blocked by:** M1-01.

**Delivers:** The owner configures a named Basileia Agent/Role and explicitly selected Provider account/model, then observes that identity/configuration on subsequent Jobs.

- [ ] Configuration survives a normal restart; private execution context is not shared with other Agents.
- [ ] Instructions and allowed Tools belong to Basileia; no employee-loop requirement or business error contract enters Archodin.
- [ ] Invalid/unsupported routes fail explicitly, credentials are redacted, and idle records do not spawn permanent tasks.

## M1-03 — Produce and test an isolated local software change

**Blocked by:** M1-02.

**Delivers:** A small software Job changes a designated fixture project and produces local patch/test artifacts within an isolated environment.

- [ ] Demonstrate an allowed edit/test and denied filesystem/credential/host-runtime escapes; the isolation design must be approved before enabling generated-code execution.
- [ ] Tool permissions, deadlines and bounded logs apply; no arbitrary host shell, push, deploy or financial action is available.
- [ ] Scripted Provider tests prove local artifacts and failure reporting without external model calls.

## M1-04 — Submit and inspect a Job in basic Agora

**Blocked by:** M1-03.

**Delivers:** The owner submits the software Job in a browser and sees its events, named Agent, terminal outcome and artifact references.

- [ ] Agora and headless inspection refer to the same run/outcome; errors do not require raw internal reasoning traces.
- [ ] Authenticated owner access is required for reads and controls; remote exposure stays disabled until secure transport is configured.
- [ ] Dashboard works without an office renderer; behavior tests do not freeze visual/component structure.

## M1-05 — Attach bounded inputs and inspect usage

**Blocked by:** M1-04.

**Delivers:** The owner submits permitted files/links/repository references and reviews provenance plus available usage/cost for the resulting Job.

- [ ] Inputs enforce agreed size/type/path and network policies; a repository link is not permission to write remotely.
- [ ] Call/attempt usage is attributable to the Job/Agent; unknown cost remains unknown rather than zero, and no unapproved paid route is selected.
- [ ] Artifact and log retrieval are bounded; private files and secrets do not appear in prompts or dashboard output accidentally.

## M1-06 — Verify the first complete local demonstration

**Blocked by:** M1-05.

**Delivers:** A repeatable acceptance walkthrough submits one small software Job, receives local changes/tests/summary and observes its usage in Agora.

- [ ] Deterministic offline acceptance covers success, refusal and failure with no external write.
- [ ] Headless library and Basileia application ownership remain separate; all earlier ticket gates have evidence.
- [ ] Limitations explicitly exclude unattended production, durable recovery and full monetary hard caps; optional live smoke requires separate credentials/spend approval.

## Decisions required before readiness

Seam and granularity are approved for planning and publication. Confirm isolation/authentication/input policy and owner acceptance of the spike recommendation before affected tickets become ready. The final demonstration gate does not prevent later work from depending on an earlier proven capability when that is its only real prerequisite.
