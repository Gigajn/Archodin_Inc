# A Graph over one shared State is the orchestration core

Multi-agent runs are modelled as a Graph: Nodes (Agents or plain functions) joined by Edges, some conditional, all reading and updating one typed State. The Strategos (supervisor), Handoff, Panel and Qadi are patterns built from Nodes and Edges rather than separate engines, so there is one runtime to test, checkpoint and stream. Agents do not message each other directly; anything one Agent needs from another goes through the State.

## Considered Options

- **Message passing between agents** (actor style): more decoupled, but harder to inspect, replay and checkpoint as one value.
- **Separate engines per pattern** (a supervisor runtime, a handoff runtime): simpler each, but duplicate the loop, streaming and persistence.
