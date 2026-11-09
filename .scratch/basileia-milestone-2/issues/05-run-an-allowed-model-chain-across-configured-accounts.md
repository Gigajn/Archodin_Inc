# 05: Run an allowed Model chain across configured accounts

**What to build:** A technical failure follows an owner-configured compatible account/model alternative with bounded retries/cooldown and complete usage attribution.

**Blocked by:** basileia-milestone-1#02, basileia-milestone-2#02, basileia-milestone-2#04, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Role defaults and Agent overrides, multiple accounts, transient failures and rate-limit cooldown are observable through run results.
- [ ] Invalid credentials/request, unsupported capability, cancellation and policy denial follow explicit classifications, not indiscriminate fallback.
- [ ] No quality-based switching or unapproved paid route is introduced; unknown accounting stays explicit. Contract tests and gate pass.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-2. Owner must confirm technical fallback classes, bounded retry/cooldown policy, account/model permissions and unknown accounting treatment; provide persistent identity, cancellation and Budget prerequisite evidence. Quality switching is not authorized.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
