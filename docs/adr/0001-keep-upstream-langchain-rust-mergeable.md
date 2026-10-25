# Keep upstream langchain-rust mergeable; build Archodin in separate crates

Archodin_Inc carries the full langchain-rust source and history (MIT, from Abraxas-365/langchain-rust via the Gigajn fork) so upstream releases can still be merged with `git merge upstream/main`. Upstream shipped three major versions in October 2026, so editing its files in place would make every merge a conflict. Archodin's own code (multi-agent orchestration, typed tools, extra providers) therefore lives in separate crates and branches and depends on langchain-rust instead of patching it; upstream files change only for fixes we would also send back as PRs. The Gigajn/langchain-rust fork is left as it is and is not a place for Archodin work.

## Considered Options

- **Hard fork** (rename and own all ~60k lines): full control, but upstream integrations stop flowing in.
- **Write from scratch**: cleanest design, but loses 13 vector stores, 6 LLM providers and the loaders.
