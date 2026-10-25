# Archodin

Archodin is a Rust library for building multi-agent systems on top of langchain-rust.

## Agents

**Agent**:
One LLM with its own instructions, tools and memory, which keeps calling tools until it produces an answer.
_Avoid_: Bot, assistant, worker

**Tool**:
A capability an Agent can call by name with structured input, returning a result the Agent reads.
_Avoid_: Function, plugin, action

**Provider**:
A source of LLM completions that an Agent talks to; an OpenAI-compatible Provider is one reached by base URL, API key and model name (9router, OpenRouter, Groq, LM Studio, vLLM).
_Avoid_: Backend, vendor, model

## Orchestration

**Graph**:
The structure a multi-agent run follows: Nodes joined by Edges, all reading and updating one State.
_Avoid_: Workflow, pipeline, flow

**Node**:
One step in a Graph, either an Agent or a plain function.
_Avoid_: Step, stage, task

**Edge**:
A link from one Node to the next; a conditional Edge picks its target from the State.
_Avoid_: Transition, route

**State**:
The single typed value shared by every Node in a Graph run, read and updated by each Node in turn.
_Avoid_: Context, memory, blackboard

**Supervisor**:
An Agent whose job is to split a task and pass the pieces to other Agents, built from Nodes and Edges.
_Avoid_: Manager, orchestrator, coordinator

**Arbiter**:
An Agent called when two or more Nodes in a run produce competing values for the same part of the State, which decides the final value.
_Avoid_: Judge, reviewer, merger

**Handoff**:
One Agent passing control of the conversation to another Agent, together with a Briefing rather than the raw history.
_Avoid_: Transfer, delegation, routing

**Briefing**:
The condensed, structured context an Agent receives on a Handoff: small enough to spare its context window, complete enough to keep the essentials of the task.
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

**Scout**:
An Agent that runs before work is assigned, checks which Skills the task needs, and finds missing ones so a person can approve them before they are installed.
_Avoid_: Skill finder, preparer, recruiter
