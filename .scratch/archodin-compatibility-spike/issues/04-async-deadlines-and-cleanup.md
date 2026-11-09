# 04: Bound and clean up asynchronous interactions

**What to build:** Hanging Provider and Tool operations terminate locally on deadline or caller cancellation, while independent operations can overlap without leaking work owned by the spike.

**Blocked by:** 03.

**Status:** ready-for-agent

- [ ] Demonstrate typed deadline and cancellation outcomes for controlled hanging Provider and Tool operations on the existing Tokio runtime.
- [ ] Prove independent calls can overlap and owned child tasks are joined or otherwise completed safely; no detached-task or nested-runtime workaround.
- [ ] Prefer controlled clocks, barriers and channels to fragile sleep-based elapsed-time assertions.
- [ ] State limitations: cancelling local futures does not guarantee remote computation or arbitrary blocking work has stopped.
- [ ] Default tests need no vendor service or credentials. Record incremental red→green evidence and the repository gate output before done.

## Comments

Owner-approved spike slice. This establishes primitive behavior, not full Job scheduling, fallback policy or production shutdown semantics.
