# Archodin routes models per Agent itself instead of relying on a gateway

Each Agent follows its own Model chain (Provider and model pairs, set per Role and overridable per Agent), moving to the next pair when one fails or hits a limit. Archodin does this routing itself rather than delegating it to an external gateway such as 9router, because the company runs on several subscriptions at once and the person must be able to say which Agent uses which one. A gateway like 9router stays usable as one OpenAI-compatible Provider inside a chain.
