# Archodin

Archodin is a Rust library for building multi-agent systems on top of langchain-rust, meant as the base of a company whose whole staff are AI Agents, run from a web console.

## Agents

**Agent**:
One LLM with its own instructions, tools and memory, which keeps calling tools until it produces an answer.
_Avoid_: Bot, assistant, worker

**Role**:
A job description from which Agents are created: instructions, Skills, Tools, a default model and its fallbacks; one Role can have many Agents.
_Avoid_: Position, persona, template

**Tool**:
A capability an Agent can call by name with structured input, returning a result the Agent reads.
_Avoid_: Function, plugin, action

**Provider**:
A source of LLM completions that an Agent talks to; an OpenAI-compatible Provider is one reached by base URL, API key and model name (9router, OpenRouter, Groq, LM Studio, vLLM).
_Avoid_: Backend, vendor, model

**Model chain**:
The ordered list of Provider and model pairs an Agent uses: the first one normally, the next ones when the one before fails or hits a limit. Set per Role, overridable per Agent.
_Avoid_: Fallback list, routing table, combo

**Budget**:
The most tokens or money an Agent or a Job may spend before it stops and waits for a person's decision.
_Avoid_: Quota, cap, limit

## Orchestration

**Job**:
A piece of work a person submits from the console, which a Supervisor breaks down and runs as a Graph.
_Avoid_: Task, request, ticket

**Graph**:
The structure a multi-agent run follows: Nodes joined by Edges, all reading and updating one State.
_Avoid_: Workflow, pipeline, flow

**Node**:
One step in a Graph: an Agent, a plain function, or a whole other Graph (which is how departments nest under a Supervisor).
_Avoid_: Step, stage, task

**Edge**:
A link from one Node to the next; a conditional Edge picks its target from the State.
_Avoid_: Transition, route

**State**:
The single typed value shared by every Node in a Graph run, read and updated by each Node in turn.
_Avoid_: Context, memory, blackboard

**Supervisor**:
An Agent whose job is to split a task and pass the pieces to other Agents, each piece started as a Flow, built from Nodes and Edges.
_Avoid_: Manager, orchestrator, coordinator

**Arbiter**:
An Agent called when two or more Nodes in a run produce competing values for the same part of the State, which decides the final value.
_Avoid_: Judge, reviewer, merger

**Panel**:
Several Agents doing the same task in parallel on purpose, after which the Arbiter picks or combines the best result.
_Avoid_: Ensemble, redundancy, voting

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
Procedural knowledge an Agent writes down from experience and refines with use, then reuses on later tasks.
_Avoid_: Prompt, recipe, playbook

**Flow**:
A Skill that is started on purpose, by a person or a Supervisor, and runs a fixed sequence of steps; a Flow may use Disciplines but never another Flow.
_Avoid_: Command, workflow, procedure

**Discipline**:
A Skill an Agent reaches for by itself whenever the task fits, holding a reusable way of working.
_Avoid_: Habit, guideline, rule

**Scout**:
An Agent that runs before work is assigned, checks which Skills the task needs, and finds missing ones so a person can approve them before they are installed.
_Avoid_: Skill finder, preparer

**Inspector**:
An Agent that checks a Skill found by the Scout, or written by an Agent, for anything unsafe before it reaches a person for approval.
_Avoid_: Auditor, scanner, guard

**Curator**:
An Agent that keeps the Skill library: it holds Skills waiting for approval, installs approved ones, and keeps their versions for rollback.
_Avoid_: Librarian, skill manager, registry
