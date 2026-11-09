# Supporting execution-plan proposals

**Status:** approved and published — the owner approved both support slices with the complete roadmap. Individual issues 01 and 02 are published under this feature. They do not block the already approved same-feature spike sequence.

## P-01 — Enforce eligible frontier selection and qualified dependencies

**Blocked by:** None.

**Delivers:** The owner/agent sees only genuinely eligible implementation candidates, with cross-feature prerequisites resolved by stable qualified identity rather than ambiguous local numbers.

- [ ] With fixture trackers, only approved ready, unblocked work is implementation-eligible; triage, information requests, human-only, claimed/in-progress and unvalidated legacy work are not offered as new implementation assignments.
- [ ] Same-feature numbers remain supported; qualified cross-feature references distinguish identically numbered tickets, detect missing/cyclic/unsupported dependency input and fail closed instead of ignoring it.
- [ ] Human-readable and structured output agree; preserve tracking history and document exact semantics with test-first evidence and the repository gate.

**Planning note:** Required before publishing executable cross-feature milestone tickets. Do not silently change the current frontier script in this planning task or claim draft proposals are already encoded there.

## P-02 — Revalidate the preserved AAHC backlog against the roadmap

**Blocked by:** None.

**Delivers:** A traceable inventory maps each legacy ticket to retained capability, required revision, deferred optional/domain work or existing completion, without executing old implementation by default.

- [ ] Inspect actual ticket/code/test evidence and preserve user edits and existing completion history; do not claim functionality absent solely from a missed search.
- [ ] For every open legacy ticket, record still relevant / needs revision / deferred and the corresponding proposed capability, then obtain owner confirmation before continuation.
- [ ] Avoid duplicate implementation when a retained module satisfies a new slice; no bulk status rewrite, source deletion or repository migration is implicit.

**Planning note:** Can proceed alongside the spike. A new slice that reuses/migrates legacy behavior must first resolve its relevant mapping; unrelated generic-library work need not wait for the whole inventory.

## Proposed approval scope

Approve these two support slices together with the six milestone proposals, or adjust them. P-01 is an execution prerequisite for machine-enforced cross-feature blocking, not a claim that every later feature needs all earlier milestones. P-02 is evidence/revalidation work, not authorization to resume all old tickets.
