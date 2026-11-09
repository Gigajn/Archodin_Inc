# Basileia and Archodin

## Document Status

Milestone 0's documentation baseline has been confirmed by the owner and closed; this does not mean the proposed runtime has been implemented or the legacy backlog fully revalidated. Confirmed product boundaries are recorded in `docs/adr/0001-archodin-basileia-boundary.md`; terminology is recorded in `GLOSSARY.md`. Remaining implementation choices below are proposals, not evidence of existing code or accepted ADRs. AAHC is the former name of Basileia; existing paths have not yet been renamed.

## Problem Statement

Kudo wants to run a company whose entire staff are AI Agents. The target system must organise Agents into Departments, hand work between them, share what they learn, control which model and Provider account each one uses, cap spending, and let a person steer it remotely. Existing implementation and dependency capabilities must be verified rather than assumed. Kudo needs to submit work from anywhere, have it broken down and done by the right Agents, approve anything risky (new Skills, hires, new Departments, overspending), and watch the company grow its own capabilities over time, without the Agents piling up tokens and context as they go.

## Solution

Basileia is a closed-source AI-company application built on Archodin, an open-source Rust library licensed under MIT. Agora is Basileia's web console. Rust is the preferred implementation language for both products. Archodin contains generic mechanisms only; company-specific agents, role instructions and workflows belong to Basileia. The runtime integration library is not yet selected; langchain-rust is a candidate, not a mandatory foundation.

A person submits a Job in the Agora. A Strategos breaks it down and runs it as a Graph: Nodes (Agents, plain functions or whole sub-Graphs for nested Departments) joined by Edges, all reading and updating one shared State, with a Checkpoint after every step and a Step limit. Agents pass work along with a Handoff carrying a structured Briefing, can work as a Panel on the same task, and any conflict between parallel results is settled by the Qadi.

Every Agent is a long-lived, named employee created from a Role. Agents of the same Role learn as one: Skills (Flows and Disciplines, `SKILL.md` format) and a cleaned-up shared knowledge store, the Akasha, belong to the Role. Before work is assigned, Ronin finds Skills the task needs (SearXNG, DuckDuckGo, GitHub); Vigil checks them for safety; Codex holds them until the person approves them in the Agora, then installs and versions them. Nakama acts as HR, proposing hires, departures and new Roles. Explorador proposes new Departments as Charters, for example around an open-source GitHub project the person links, which then work through forks, branches and pull requests.

Each Agent follows a Model chain of Provider accounts and models (any OpenAI-compatible Provider such as 9router, plus native ones), falling back on errors, rate limits or timeouts. Budgets stop Agents and Jobs that overspend until the person decides.

## Confirmed Product Scope

- The initial deployment serves one owner on a local workstation or mini-server. Remote Agora access must be authenticated; multi-tenant SaaS and customer billing are not initial requirements.
- After the compatibility spike, the first end-to-end demonstration is a bounded software Job in a designated project workspace. It produces local changes, test results, a summary and token/cost accounting, without automatic pushes, deployments or financial transactions. Execution isolation and detailed acceptance criteria belong to the feature spec.

- Departments are configurable. Software/ML, Trading, Media/Affiliate and Customer Service from the former AAHC spec are examples or future candidates, not mandatory subsystems or startup dependencies.
- The first operational Agora is a dashboard for Jobs, events, results, controls and approvals. A 2D pixel-art office remains a later product target, using the same underlying events and controls rather than a separate execution engine.
- Cost is measured and bounded by the owner's configured policies. Free tiers and local models may be preferred, but neither zero-cost operation nor uninterrupted failover is promised. An unavailable free route does not authorize an unapproved paid route; the Job waits or requests a decision according to its Budget policy.

## User Stories

1. As Kudo, I want to submit a Job from the Agora in a web browser, so that I can give the company work from anywhere.
2. As Kudo, I want to attach links, files and a GitHub repository URL to a Job, so that the Agents have the material they need.
3. As Kudo, I want a Strategos to break my Job into pieces and assign each to the right Agents, so that I don't have to plan the work myself.
4. As Kudo, I want to follow a running Job step by step in the Agora, so that I can see which Agent is doing what.
5. As Kudo, I want to see a Job's final result and the decisions made along the way, so that I can trust or correct it.
6. As Kudo, I want to cancel a running Job, so that I can stop work that is no longer needed.
7. As Kudo, I want a run to stop after its Step limit (25 by default, configurable), so that a looping Graph cannot run forever.
8. As Kudo, I want every step's State saved as a Checkpoint, so that a Job can resume after a crash or restart.
9. As Kudo, I want a Job to pause at a Checkpoint and wait for my approval when a step needs it, so that I stay in control of important actions.
10. As a Strategos, I want to hand each piece of work to an Agent as a Flow with fixed steps, so that work is done the same structured way every time.
11. As a Strategos, I want to lead other Strategoi, so that Departments can nest (for example CEO, Head of Engineering, Engineers).
12. As a Graph author, I want a Node to be an Agent, a plain function or a whole other Graph, so that I can compose Departments from smaller parts.
13. As a Graph author, I want conditional Edges that choose the next Node from the State, so that routing can be done in code when an LLM is not needed.
14. As a Graph author, I want all Nodes to share one typed State, so that Agents never need to message each other directly.
15. As a Graph author, I want several Nodes to run in parallel, so that independent work finishes sooner.
16. As a Graph author, I want list fields in the State (such as messages) to be appended to rather than overwritten when parallel Nodes write them, so that no work is lost.
17. As Kudo, I want the Qadi to decide the final value when two or more Nodes produce competing values for the same part of the State, so that conflicts are settled by judgement instead of by whichever finished last.
18. As a Strategos, I want to run a Panel of several Agents on the same task, so that the Qadi can pick or combine the best result.
19. As an Agent, I want to hand a conversation to another Agent with a Briefing in fixed sections (Goal, Done so far, Decisions, Open questions, Related material), so that the receiver gets the essentials without the full history filling its context.
20. As an Agent receiving a Handoff, I want to fetch the full history from the State through a Tool when I need it, so that nothing is lost by the Briefing being short.
21. As Kudo, I want every Agent to have its own name shown next to its Role, so that I can tell them apart.
22. As Kudo, I want an Agent's git branches to start with its name (`<agent-name>/<topic>`), so that it is clear which Agent made a change.
23. As Kudo, I want Agents to be long-lived and keep what they learn between Jobs, so that the company gets better over time.
24. As Kudo, I want to define a Role with instructions, Tools, Skills and a Model chain, so that I can create several Agents from the same job description.
25. As Kudo, I want all Agents of a Role to gain a Skill as soon as it is approved for any one of them, so that the Role learns as one, like an ant colony.
26. As Kudo, I want Skills to reach other Roles only when Codex marks them shared, so that unrelated Roles are not flooded with Skills they do not need.
27. As an Agent, I want to read my Role's Akasha, so that I know the facts about clients, projects and decisions my colleagues already learned.
28. As Kudo, I want the Akasha kept small and free of duplicates, so that reading it costs few tokens.
29. As an Agent, I want my ongoing conversation to stay my own, so that colleagues' work in progress does not fill my context.
30. As Kudo, I want to set a Model chain per Role and override it per Agent, so that I decide which subscription and model each Agent uses.
31. As Kudo, I want a Provider to have several accounts (for example two API keys for the same vendor), so that I can spread Agents across my subscriptions.
32. As Kudo, I want any OpenAI-compatible Provider (9router, OpenRouter, Groq, LM Studio, vLLM) to work by base URL, API key and model name, so that I am not limited to built-in vendors.
33. As Kudo, I want an Agent to move to the next pair in its Model chain on an error, a rate limit or a timeout, so that work continues when one Provider fails.
34. As Kudo, I want poor-quality answers handled by a Panel and the Qadi rather than by switching models, so that fallbacks stay predictable.
35. As Kudo, I want a Budget per Agent and per Job, so that spending stays under control.
36. As Kudo, I want a Job or Agent that hits its Budget to stop and ask me in the Agora whether to continue, so that I decide on any overspend.
37. As Kudo, I want to see token use and cost per Agent, Job and Department, so that I know where money goes.
38. As Kudo, I want Ronin to check which Skills a task needs before the work is assigned, so that Agents start with the right know-how.
39. As Kudo, I want Ronin to search for missing Skills for free without a login (self-hosted SearXNG when configured, DuckDuckGo otherwise, and the GitHub API), so that discovery costs nothing.
40. As Kudo, I want to optionally log in with GitHub in the Agora, so that Ronin gets higher rate limits and code search.
41. As Kudo, I want Skills from repositories outside my trusted list labelled unverified and checked more strictly, so that unknown sources get extra scrutiny.
42. As Kudo, I want Agents to write a new Skill or refine one after a successful run with at least 5 Tool calls, or after I correct them, so that the company learns from experience.
43. As Kudo, I want Vigil to check every new or changed Skill with static rules and then an LLM review, so that dangerous instructions or scripts are caught before they reach me.
44. As Kudo, I want a rejected Skill to show Vigil's reason in the Agora, so that I understand why it was blocked.
45. As Kudo, I want Codex to hold every new or changed Skill, downloaded or self-written, until I approve it, so that nothing is installed behind my back.
46. As Kudo, I want approvals grouped per Role with Vigil's summary, and the option to approve a group at once, so that a long queue does not take all my time.
47. As Kudo, I want scripts inside a Skill to stay disabled until I approve them separately, so that installing a Skill never runs code by itself.
48. As Kudo, I want every Skill change kept as a version I can roll back, so that a bad update is easy to undo.
49. As Kudo, I want automatic updates limited to Skills and the Akasha, so that Agents never change Archodin's code, Provider configuration or Tool permissions on their own.
50. As Kudo, I want Skills split into Flows (started on purpose, fixed steps, may use Disciplines but never another Flow) and Disciplines (used by an Agent whenever the task fits), so that the company works in the structured way the mattpocock skills do.
51. As Kudo, I want Nakama to watch workload and Budgets and propose hiring or letting go of Agents, so that the staff size follows the work.
52. As Kudo, I want Nakama to draft new Roles when no existing Role fits, so that the company can take on new kinds of work.
53. As Kudo, I want every hire, departure and new Role to wait for my approval in the Agora, so that I control the company's shape and cost.
54. As Kudo, I want Explorador to look for new opportunities only in the fields I set in the Agora, so that the company grows in directions I chose.
55. As Kudo, I want Explorador to propose each new Department as a Charter (goal, where its work comes from, Roles, Budget), so that I can judge it before anything is created.
56. As Kudo, I want to paste a GitHub repository link in the Agora as the start of a Charter, so that I can point the company at an open-source project I like.
57. As Kudo, I want an open-source Department to turn the repository's issues into Jobs automatically within its Budget, so that it keeps contributing without me feeding it.
58. As Kudo, I want Jobs I submit to take priority over Jobs created from issues, so that my requests come first.
59. As Kudo, I want open-source work done through a fork, a branch and a pull request, never a direct push to another project's main branch, so that maintainers keep control of their projects.
60. As Kudo, I want pull requests to open-source projects to state that the contribution was made with AI, so that we respect projects that require disclosure.
61. As a library user, I want the core (Graph, Agents, Providers, Skills) usable without the Agora, so that Archodin can run headless or behind another interface.
62. As a library user, I want the approval step exposed as an interface that the Agora implements, so that other front ends can approve too.
63. As a library maintainer, I want Archodin's integration dependencies selected through a focused compatibility spike, so that Provider and Tool support fits the library's needs without an unverified upstream constraint.

## Implementation Decisions

- Proposed library crates: `archodin` (umbrella), `archodin-graph` (Graph, State, Checkpoint), and `archodin-tools` (typed Tools); further crates may split out as modules deepen. Basileia and Agora remain outside the Archodin library; their crate layout is not yet decided. The future repositories will be separate; development remains local until the owner separates and pushes them manually at the end (ADR 0002).
- Graph runtime: Nodes, conditional Edges, one typed State per run with a merge rule per field (overwrite by default, append for lists, an application-supplied resolver for competing values; Basileia may supply Qadi), parallel Nodes, sub-Graphs as Nodes, a configurable Step limit (default 25), and a Checkpoint after each step with an in-memory store first and SQLite/Postgres later.
- Strategos, Qadi, Ronin, Vigil, Codex, Nakama and Explorador are Basileia Agents built from Roles, not built-in Archodin agents or special engines; Panel and Handoff use generic Graph mechanisms.
- Agents are persistent records (name, Role, Model chain override, Budget, own conversation), not per-Job objects.
- Skills are stored as `SKILL.md` (agentskills.io format) per Role, versioned, with a shared flag; installation and script enablement always go through the approval interface.
- The Akasha is a per-Role store that Codex keeps deduplicated and condensed.
- Providers: an OpenAI-compatible Provider first (base URL, key, model) using an implementation to be selected by the compatibility spike; multiple accounts per Provider; a Model chain per Role overridable per Agent, advancing only on error, rate limit or timeout.
- Tools get a typed interface (structured input with a generated schema, `Send + Sync` errors) with any external-library adapter determined by the compatibility spike, and independent Tool calls in one step run concurrently.
- Search for Skills: self-hosted SearXNG when configured, a DuckDuckGo integration otherwise (implementation not yet selected), plus the GitHub API with an optional OAuth login from the Agora.
- Approvals (Skills, scripts, hires, Roles, Charters, Budget overruns) go through one approval interface in the core; the Agora implements it as a web page grouped per Role.
- GitHub work for Departments uses the person's GitHub account, fork plus `<agent-name>/<topic>` branches plus pull requests, with AI disclosure in the PR body.
- Archodin-owned code is open-source under MIT; Basileia-owned application code is closed-source. Third-party code retains its applicable licenses and notices. See `docs/adr/0001-archodin-basileia-boundary.md`.
- Before building the full Graph, run a focused compatibility spike covering Providers, Tools, async runtime, error handling and offline tests. langchain-rust may be retained, replaced or omitted based on evidence; no particular dependency or fork strategy is preselected.
- The complete proposed implementation sequence and Goal-to-milestone coverage are maintained in `.scratch/basileia-roadmap/roadmap.md`, with one spec and proposed ticket breakdown per milestone. The approved compatibility spike is a prerequisite within Milestone 1, not the complete remaining roadmap.

## Testing Decisions

- Test public behavior, not internal structure. The owner-approved spike uses generic Provider and typed Tool interfaces. Later milestones propose the public Job/Graph execution interface (events and final State/artifacts), the approval interface and matching Agora acceptance checks; those detailed seams remain subject to roadmap approval.
- Providers, GitHub and search engines are replaced with scripted fakes that return fixed responses, so tests need no network and cost nothing.
- Candidate prior art: langchain-rust's offline provider testing patterns. The previous claim of 497 passing offline tests is not verified for this checkout and is not an acceptance criterion.
- Each build phase lands with tests at that seam: Graph routing, merge rules, parallel runs, Step limit and Checkpoint resume; Model chain fallback and Budget stops; Handoff Briefing contents and Qadi decisions; Skill approval, rollback and Vigil rejections.

## Out of Scope

- Selecting, importing or forking an integration library without evaluating the agreed compatibility requirements.
- Using consumer chat subscriptions (Claude Pro, ChatGPT Plus and similar) through unofficial routes; Providers are reached through API keys or OpenAI-compatible endpoints.
- Agents changing Archodin's code, Provider configuration or Tool permissions on their own.
- Publishing Basileia's proprietary application code, company-specific agents, role instructions or workflows as part of Archodin. Archodin is open-source; repository separation and pushes will be performed manually by the owner at the end of local program development (ADR 0002).
- Running scripts from Skills without a separate approval.

## Further Notes

- Vocabulary follows `GLOSSARY.md`. The confirmed product boundary is ADR 0001 in `docs/adr/`; the former references to ADRs 0001–0005 did not correspond to files in this checkout and have been removed.
- `docs/agents/domain.md` defines which documents own vision, terminology, architectural decisions, feature requirements and implementation evidence.
- The former reference to `docs/analysis/langchain-rust.md` did not correspond to a file in this checkout. Dependency and CI claims must be established from the selected integration and actual configuration rather than inherited from that reference.
