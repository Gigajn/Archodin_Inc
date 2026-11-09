# 06: Verify authenticated remote control and recovery

**What to build:** An owner reconnects to Agora, inspects attributable Job/Agent/Department usage and operates cancel/approval/recovery consistently with headless execution.

**Blocked by:** basileia-milestone-1#04, basileia-milestone-2#01, basileia-milestone-2#02, basileia-milestone-2#03, basileia-milestone-2#04, basileia-milestone-2#05, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Authentication and session/control protections apply to reads, streaming events and writes; insecure remote exposure is refused under the approved deployment policy.
- [ ] Restart/concurrent approval/Budget/cancel walkthroughs preserve invariant outcomes and bounded logs without secrets.
- [ ] Operational limits, unknown spend and recovery limitations are visible; the gate and offline acceptance evidence are recorded.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must approve the authentication/session and secure remote-access/deployment method, checkpoint compatibility, run/Step transitions, approval expiry and monetary reservation/pricing-gap policies; provide all listed control/recovery prerequisites and a plan for restart/concurrency/redaction evidence.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
