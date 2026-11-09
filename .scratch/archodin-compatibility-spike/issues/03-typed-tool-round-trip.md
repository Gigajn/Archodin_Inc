# 03: Complete one validated typed Tool round trip

**What to build:** One bounded Provider interaction can request an explicitly registered harmless Tool and receive its correlated structured result, with invalid requests rejected before execution.

**Blocked by:** 02.

**Status:** ready-for-agent

- [ ] Exercise structured input validation, typed output and call/result correlation through the public Provider/Tool interfaces.
- [ ] Invalid input and unknown Tool names produce typed outcomes with observable proof that the Tool did not execute.
- [ ] A model-provided Tool name does not grant authority beyond the explicit registration. Use harmless scripted behavior, not arbitrary scripts or workspace mutation.
- [ ] Keep the scenario bounded; introduce no recursive agent loop, Graph or Basileia-specific agent behavior.
- [ ] Use default offline tests, record red→green evidence one behavior at a time, and record the repository gate output before done.

## Comments

Owner-approved spike slice. Observe behavior, not private registry representation or vendor SDK internals.
