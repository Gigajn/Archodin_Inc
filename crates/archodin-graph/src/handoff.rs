//! Handoff: one Agent passes the conversation to another with a Briefing.
//!
//! The receiving Agent's context holds the [`Briefing`] (fixed sections), not the
//! sender's raw history. When it needs more, it calls the history Tool, which
//! returns the full conversation as it stands in the State.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{AgentError, AgentNode, AgentTool, GraphBuilder, TypedTool};

/// Name of the Tool that returns the full conversation.
pub const HISTORY_TOOL: &str = "conversation_history";

/// One turn of a conversation: who said it and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    speaker: String,
    text: String,
}

impl Turn {
    /// A turn spoken by `speaker`.
    pub fn new(speaker: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            speaker: speaker.into(),
            text: text.into(),
        }
    }

    /// Who spoke.
    pub fn speaker(&self) -> &str {
        &self.speaker
    }

    /// What was said.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// The condensed context an Agent receives on a Handoff, always in the same
/// sections: Goal, Done so far, Decisions, Open questions, Related material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Briefing {
    goal: String,
    done_so_far: Vec<String>,
    decisions: Vec<String>,
    open_questions: Vec<String>,
    related_material: Vec<String>,
}

impl Briefing {
    /// A Briefing for `goal`, with the other sections empty.
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            done_so_far: Vec::new(),
            decisions: Vec::new(),
            open_questions: Vec::new(),
            related_material: Vec::new(),
        }
    }

    /// Adds an item to "Done so far".
    pub fn done_so_far(mut self, item: impl Into<String>) -> Self {
        self.done_so_far.push(item.into());
        self
    }

    /// Adds an item to "Decisions".
    pub fn decision(mut self, item: impl Into<String>) -> Self {
        self.decisions.push(item.into());
        self
    }

    /// Adds an item to "Open questions".
    pub fn open_question(mut self, item: impl Into<String>) -> Self {
        self.open_questions.push(item.into());
        self
    }

    /// Adds an item to "Related material".
    pub fn related(mut self, item: impl Into<String>) -> Self {
        self.related_material.push(item.into());
        self
    }

    /// The Briefing as text, with all five sections in a fixed order.
    pub fn render(&self) -> String {
        fn list(items: &[String]) -> String {
            if items.is_empty() {
                "- (none)".to_string()
            } else {
                items
                    .iter()
                    .map(|i| format!("- {i}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        format!(
            "## Goal\n{}\n\n## Done so far\n{}\n\n## Decisions\n{}\n\n## Open questions\n{}\n\n## Related material\n{}\n\n\
             The full conversation is available through the `{HISTORY_TOOL}` tool if you need it.",
            self.goal,
            list(&self.done_so_far),
            list(&self.decisions),
            list(&self.open_questions),
            list(&self.related_material),
        )
    }
}

/// What a Handoff carries: the Briefing for the receiver, and the full
/// conversation it can fetch through the history Tool.
#[derive(Debug, Clone)]
pub struct Handoff {
    briefing: Briefing,
    history: Vec<Turn>,
}

impl Handoff {
    /// A Handoff with `briefing`, backed by the full `history`.
    pub fn new(briefing: Briefing, history: Vec<Turn>) -> Self {
        Self { briefing, history }
    }
}

#[derive(Deserialize, JsonSchema)]
struct NoInput {}

#[derive(Debug, thiserror::Error)]
#[error("history unavailable")]
struct NoError;

struct HistoryTool(Arc<Vec<Turn>>);

impl TypedTool for HistoryTool {
    type Input = NoInput;
    type Error = NoError;
    fn name(&self) -> String {
        HISTORY_TOOL.into()
    }
    fn description(&self) -> String {
        "Returns the full conversation so far, turn by turn".into()
    }
    async fn call(&self, _: NoInput) -> Result<String, NoError> {
        Ok(self
            .0
            .iter()
            .map(|t| format!("{}: {}", t.speaker, t.text))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a Node where `receiver` takes over the conversation: `read` builds the
    /// [`Handoff`] from the State, the receiver is given the Briefing (not the
    /// history) plus a history Tool, and `write` puts its answer back in the State.
    pub fn handoff_node<R, W>(self, name: &str, receiver: AgentNode, read: R, write: W) -> Self
    where
        R: Fn(&S) -> Handoff + Send + Sync + 'static,
        W: Fn(S, Result<String, AgentError>) -> S + Send + Sync + 'static,
    {
        let (read, write) = (Arc::new(read), Arc::new(write));
        self.node(name, move |state: S| {
            let (receiver, read, write) = (receiver.clone(), read.clone(), write.clone());
            async move {
                let handoff = read(&state);
                let receiver =
                    receiver.tool(AgentTool::typed(HistoryTool(Arc::new(handoff.history))));
                let answer = receiver.answer(&handoff.briefing.render()).await;
                write(state, answer)
            }
        })
    }
}
