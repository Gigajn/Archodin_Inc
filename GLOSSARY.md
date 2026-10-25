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

**Handoff**:
One Agent passing control of the conversation to another Agent.
_Avoid_: Transfer, delegation, routing
