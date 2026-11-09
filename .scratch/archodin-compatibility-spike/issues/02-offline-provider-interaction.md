# 02: Complete one offline Provider interaction

**What to build:** A caller submits a request through a generic Provider contract and receives a normalized response or typed failure from a scripted offline adapter, without knowing vendor SDK types.

**Blocked by:** 01.

**Status:** ready-for-agent

- [ ] Demonstrate a complete successful request/response and actionable typed failure through the public interface; do not deliver only an empty trait.
- [ ] Distinguish determinable authentication, rate-limit, transport and invalid-response failures, and report unsupported capabilities explicitly.
- [ ] Represent missing usage as unavailable rather than zero; requests, errors and diagnostics disclose no credentials.
- [ ] Use scripted injection so default tests require no external network, API key or inference spend. No Basileia Agent, employee model or legacy business error codes enter the generic contract.
- [ ] Work one behavior at a time with recorded red→green evidence, preserve existing behavior, and record the repository gate output before done.

## Comments

Owner-approved spike slice. No integration dependency or final crate layout is preselected. Blocking ticket approval and completion are required before work starts.
