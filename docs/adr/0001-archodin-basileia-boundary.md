# Separate the Archodin library from the Basileia application

Archodin is an open-source Rust library licensed under MIT. Basileia, formerly called AAHC, is the closed-source AI-company application built on Archodin; Agora is Basileia's web console. Rust is the preferred implementation language for both products, with exceptions requiring a concrete need.

The library/application boundary is deliberate: Archodin supplies generic mechanisms for graphs, agents, roles, tools, providers, handoffs, budgets, skills and approval interfaces. Basileia owns the company-specific agent definitions, role instructions, prompts, workflows, organizational policies and Agora. Basileia's agents, including Strategos, Qadi, Ronin, Vigil, Codex, Nakama and Explorador, must not become built-in Archodin agents or leak into its library assets.

This separation allows other developers to build applications on Archodin without requiring Basileia or receiving its proprietary company behavior. It replaces the earlier proposal that Archodin itself would be proprietary. Third-party code retains its applicable licenses and notices; the MIT decision applies to Archodin-owned code, not the entire repository indiscriminately.

Repository separation and its timing are recorded in ADR 0002. Migration of existing AAHC code remains undecided. This decision does not authorize deleting legacy code or changing license metadata before code ownership boundaries are reviewed.
