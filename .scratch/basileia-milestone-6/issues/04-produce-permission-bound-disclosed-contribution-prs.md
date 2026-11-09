# 04: Produce permission-bound, disclosed contribution PRs

**What to build:** An eligible issue Job produces a reviewed contribution through a fork, `<agent-name>/<topic>` branch and AI-disclosed PR, with its outcome visible in Agora.

**Blocked by:** basileia-milestone-6#03, basileia-milestone-1#03, basileia-roadmap#01

**Status:** needs-info

## Acceptance Criteria

- [ ] Scripted GitHub tests prove allowed-account/repository checks, target contribution-policy handling and refusal of direct upstream-main writes or insufficient permissions.
- [ ] PR output names the Agent and includes AI disclosure; prohibited or unclear project policies pause for an owner decision rather than silent submission.
- [ ] Retries, rate limits and restart after an ambiguous write reconcile persistent action identity before retry, avoiding duplicate forks/branches/PRs and leaking no credentials.

**Goals coverage:** 22, 59–60, supporting 5, 8–9.

## Comments

Owner approved the complete roadmap, specs/seams and slice breakdown after the final approval question, then confirmed “tdd okey”. This records planning and ticket-publication approval, not resolution of runtime policies or authorization to implement needs-info work.

### Readiness

Planning approved. Spec reference: basileia-milestone-6. Owner must approve GitHub account/repository scopes, contribution/AI-policy checks and ambiguous-write reconciliation; provide bounded issue-intake and isolated local-change prerequisite evidence. Live external writes need explicit owner permission; scripted success alone does not certify the integration.

Before promotion to ready-for-agent, record the required owner decisions and prerequisite acceptance evidence here, verify every listed capability blocker and basileia-roadmap#01 is complete, and explicitly confirm readiness. P-01 is a publication/eligibility gate, not a substitute for capability or policy evidence.
