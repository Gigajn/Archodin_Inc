# Archodin

Archodin is a Rust library for building multi-agent systems on top of langchain-rust, meant as the base of a company whose whole staff are AI Agents, run from the Agora web console. Archodin's own Agents carry names borrowed from other languages; each definition says what the Agent does.

## Company

**Agent**:
A long-lived AI employee: one LLM with instructions, Tools and memory, created from a Role, which keeps calling Tools until it produces an answer and carries what it learns from one Job to the next.
_Avoid_: Bot, assistant, worker, employee

**Tool**:
A capability an Agent can call by name with structured input, returning a result the Agent reads.
_Avoid_: Function, plugin, action

**Role**:
A job description from which Agents are created: instructions, Skills, Tools, a Model chain and an Akasha; one Role can have many Agents, and they all learn as one.
_Avoid_: Position, persona, template

**Nakama**:
The Agent that acts as HR: it watches workload and Budgets, proposes hiring Agents from a Role or letting them go, and drafts new Roles, all for a person to approve.
_Avoid_: HR, recruiter, staffing

**Job**:
A piece of work a person submits from the Agora, which a Strategos breaks down and runs as a Graph.
_Avoid_: Task, request, ticket

**Budget**:
The most tokens or money an Agent or a Job may spend before it stops and waits for a person's decision.
_Avoid_: Quota, cap, limit

**Agora**:
The web console where a person submits Jobs, follows them, and approves Skills, hires and Budget overruns from anywhere.
_Avoid_: Dashboard, admin panel, UI

## Models

**Provider**:
A source of LLM completions; an OpenAI-compatible Provider is one reached by base URL, API key and model name (9router, OpenRouter, Groq, LM Studio, vLLM). One Provider may have several accounts.
_Avoid_: Backend, vendor, model

**Model chain**:
The ordered list of Provider account and model pairs an Agent uses: the first one normally, the next ones only on an error, a rate limit or a timeout. Set per Role, overridable per Agent.
_Avoid_: Fallback list, routing table, combo

## Orchestration

**Graph**:
The structure a Job follows: Nodes joined by Edges, all reading and updating one State.
_Avoid_: Workflow, pipeline, flow

**Node**:
One step in a Graph: an Agent, a plain function, or a whole other Graph (which is how departments nest under a Strategos).
_Avoid_: Step, stage, task

**Edge**:
A link from one Node to the next; a conditional Edge picks its target from the State.
_Avoid_: Transition, route

**State**:
The single typed value shared by every Node in a Graph run, read and updated by each Node in turn.
_Avoid_: Context, memory, blackboard

**Strategos**:
The Agent that acts as supervisor: it splits a Job and hands each piece to other Agents as a Flow; a Strategos may lead other Strategoi.
_Avoid_: Supervisor, manager, orchestrator

**Panel**:
Several Agents doing the same task in parallel on purpose, after which the Qadi picks or combines the best result.
_Avoid_: Ensemble, redundancy, voting

**Qadi**:
The Agent called when two or more Nodes produce competing values for the same part of the State, which decides the final value.
_Avoid_: Arbiter, judge, reviewer

**Handoff**:
One Agent passing control of the conversation to another Agent, together with a Briefing rather than the raw history.
_Avoid_: Transfer, delegation, routing

**Briefing**:
The condensed context an Agent receives on a Handoff, always in the same sections: Goal, Done so far, Decisions, Open questions, Related material.
_Avoid_: Summary, context dump

**Step limit**:
The most Node executions one Graph run may take before it stops (default 25).
_Avoid_: Recursion limit, max iterations

**Checkpoint**:
A saved copy of the State after a step, from which a run can resume or wait for a person's approval.
_Avoid_: Snapshot, savepoint

## Learning

**Skill**:
Procedural knowledge written down from experience and refined with use, owned by a Role so every Agent of that Role gains it at once.
_Avoid_: Prompt, recipe, playbook

**Flow**:
A Skill that is started on purpose, by a person or a Strategos, and runs a fixed sequence of steps; a Flow may use Disciplines but never another Flow.
_Avoid_: Command, workflow, procedure

**Discipline**:
A Skill an Agent reaches for by itself whenever the task fits, holding a reusable way of working.
_Avoid_: Habit, guideline, rule

**Akasha**:
The cleaned-up knowledge a Role's Agents share (facts about clients, projects and decisions), kept small and free of duplicates so it costs few tokens; each Agent's ongoing conversation stays its own.
_Avoid_: Knowledge base, shared memory, wiki

**Ronin**:
The Agent that acts as scout: before work is assigned it checks which Skills a task needs and searches for missing ones.
_Avoid_: Scout, skill finder

**Vigil**:
The Agent that inspects every new or changed Skill for anything unsafe before it reaches a person for approval.
_Avoid_: Inspector, auditor, scanner

**Codex**:
The Agent that keeps the Skill library and the Akasha: it holds Skills waiting for approval, installs approved ones, keeps their versions for rollback, and marks Skills shared across Roles.
_Avoid_: Curator, librarian, registry
