//! Roles and long-lived named Agents.
//!
//! A [`Role`] is a job description (instructions, Tools, a Skills slot, a Model
//! chain, a Budget). A [`Roster`] creates [`Agent`]s from Roles: each has a unique
//! name shown next to its Role, its own ongoing conversation that survives from
//! one Job to the next, and an optional Model chain that overrides the Role's.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::lock::Mutex as AsyncMutex;
use langchain_rust::schemas::Message;

use crate::locks::mutex;
use crate::{AgentError, AgentNode, AgentTool, ChainEvent, GraphBuilder, ModelChain, Provider};

/// A job description from which Agents are created.
#[derive(Clone)]
pub struct Role {
    name: String,
    instructions: String,
    tools: Vec<AgentTool>,
    skills: Vec<String>,
    model_chain: Vec<Provider>,
    budget_tokens: Option<u64>,
}

impl Role {
    /// A Role called `name` whose Agents follow `instructions`.
    pub fn new(name: impl Into<String>, instructions: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instructions: instructions.into(),
            tools: Vec::new(),
            skills: Vec::new(),
            model_chain: Vec::new(),
            budget_tokens: None,
        }
    }

    /// Gives the Role's Agents a Tool.
    pub fn tool(mut self, tool: AgentTool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Adds a Skill to the Role's Skills slot, by name.
    pub fn skill(mut self, skill: impl Into<String>) -> Self {
        self.skills.push(skill.into());
        self
    }

    /// Sets the Role's Model chain: the Provider account and model pairs, in order.
    pub fn model_chain(mut self, chain: Vec<Provider>) -> Self {
        self.model_chain = chain;
        self
    }

    /// Sets the most tokens one Agent of this Role may spend.
    pub fn budget_tokens(mut self, tokens: u64) -> Self {
        self.budget_tokens = Some(tokens);
        self
    }

    /// The Role's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The Role's instructions.
    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    /// The names of the Skills in the Role's Skills slot.
    pub fn skills(&self) -> &[String] {
        &self.skills
    }

    /// The Role's token Budget, if it has one.
    pub fn budget(&self) -> Option<u64> {
        self.budget_tokens
    }
}

/// Why a hire failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RosterError {
    /// No Role has this name.
    #[error("unknown role `{0}`")]
    UnknownRole(String),
    /// Another Agent already has this name.
    #[error("an agent named `{0}` already exists")]
    NameTaken(String),
    /// The name is empty or has spaces or slashes, so it cannot start a git branch.
    #[error("invalid agent name `{0}`")]
    InvalidName(String),
}

#[derive(Default)]
struct RosterInner {
    roles: HashMap<String, Role>,
    agents: Vec<Agent>,
}

/// The company's Roles and the Agents created from them.
#[derive(Clone, Default)]
pub struct Roster {
    inner: Arc<Mutex<RosterInner>>,
}

impl Roster {
    /// An empty Roster.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a Role, replacing any Role with the same name.
    pub fn add_role(&self, role: Role) {
        mutex(&self.inner).roles.insert(role.name.clone(), role);
    }

    /// Creates an Agent called `name` from the Role called `role`.
    pub fn hire(&self, role: &str, name: &str) -> Result<Agent, RosterError> {
        let mut inner = mutex(&self.inner);
        if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '/') {
            return Err(RosterError::InvalidName(name.into()));
        }
        let role = inner
            .roles
            .get(role)
            .cloned()
            .ok_or_else(|| RosterError::UnknownRole(role.into()))?;
        if inner.agents.iter().any(|a| a.name() == name) {
            return Err(RosterError::NameTaken(name.into()));
        }
        let agent = Agent {
            state: Arc::new(AgentState {
                name: name.into(),
                role,
                chain_override: Mutex::new(None),
                on_event: Mutex::new(None),
                conversation: AsyncMutex::new(Vec::new()),
                history_len: Mutex::new(0),
            }),
        };
        inner.agents.push(agent.clone());
        Ok(agent)
    }

    /// The Role called `name`, if there is one.
    pub fn role(&self, name: &str) -> Option<Role> {
        mutex(&self.inner).roles.get(name).cloned()
    }

    /// Every Role, ordered by name.
    pub fn roles(&self) -> Vec<Role> {
        let mut roles: Vec<Role> = mutex(&self.inner).roles.values().cloned().collect();
        roles.sort_by(|a, b| a.name.cmp(&b.name));
        roles
    }

    /// Lets the Agent called `name` go, returning it if there was one.
    pub fn release(&self, name: &str) -> Option<Agent> {
        let mut inner = mutex(&self.inner);
        let at = inner.agents.iter().position(|a| a.name() == name)?;
        Some(inner.agents.remove(at))
    }

    /// The Agent called `name`, if there is one.
    pub fn agent(&self, name: &str) -> Option<Agent> {
        mutex(&self.inner)
            .agents
            .iter()
            .find(|a| a.name() == name)
            .cloned()
    }

    /// Every Agent, in the order they were hired.
    pub fn agents(&self) -> Vec<Agent> {
        mutex(&self.inner).agents.clone()
    }
}

/// The most messages an Agent's conversation keeps. Older turns are dropped
/// so one long-lived Agent cannot grow memory without bound (KR-06/SD-05):
/// every [`Agent::ask`] adds two turns, so without a cap a busy Agent's
/// history -- and every request built from it -- grows forever.
pub const MAX_HISTORY_MESSAGES: usize = 100;

/// Drops the oldest turns of `history` until at most
/// [`MAX_HISTORY_MESSAGES`] remain.
fn prune_history(history: &mut Vec<Message>) {
    if history.len() > MAX_HISTORY_MESSAGES {
        history.drain(..history.len() - MAX_HISTORY_MESSAGES);
    }
}

type Listener = Arc<dyn Fn(ChainEvent) + Send + Sync>;

struct AgentState {
    name: String,
    role: Role,
    chain_override: Mutex<Option<Vec<Provider>>>,
    on_event: Mutex<Option<Listener>>,
    conversation: AsyncMutex<Vec<Message>>,
    history_len: Mutex<usize>,
}

/// A long-lived named employee created from a [`Role`]. Cloning gives another
/// handle to the same Agent.
#[derive(Clone)]
pub struct Agent {
    state: Arc<AgentState>,
}

impl Agent {
    /// The Agent's unique name.
    pub fn name(&self) -> &str {
        &self.state.name
    }

    /// The name of the Agent's Role.
    pub fn role(&self) -> &str {
        &self.state.role.name
    }

    /// The name shown to people: the Agent's name next to its Role.
    pub fn title(&self) -> String {
        format!("{} ({})", self.name(), self.role())
    }

    /// Overrides the Role's Model chain for this Agent only.
    pub fn set_model_chain(&self, chain: Vec<Provider>) {
        *mutex(&self.state.chain_override) = Some(chain);
    }

    /// Goes back to the Role's Model chain.
    pub fn clear_model_chain(&self) {
        *mutex(&self.state.chain_override) = None;
    }

    /// Reports each fallback along this Agent's Model chain to `listener`.
    pub fn on_event(&self, listener: impl Fn(ChainEvent) + Send + Sync + 'static) {
        *mutex(&self.state.on_event) = Some(Arc::new(listener));
    }

    fn model_chain(&self) -> Vec<Provider> {
        mutex(&self.state.chain_override)
            .clone()
            .unwrap_or_else(|| self.state.role.model_chain.clone())
    }

    /// How many messages the Agent's own conversation holds.
    pub fn history_len(&self) -> usize {
        *mutex(&self.state.history_len)
    }

    /// The branch name for this Agent's work on `topic`: `<agent-name>/<topic>`.
    pub fn branch(&self, topic: &str) -> String {
        branch_name(self.name(), topic)
    }

    /// Answers `request` as part of the Agent's ongoing conversation. A failed
    /// call leaves the conversation as it was.
    pub async fn ask(&self, request: &str) -> Result<String, AgentError> {
        self.ask_with_usage(request).await.map(|(answer, _)| answer)
    }

    /// The most tokens this Agent may spend, from its Role.
    pub fn budget_tokens(&self) -> Option<u64> {
        self.state.role.budget()
    }

    /// Like [`Agent::ask`], also returning the usage, priced by the Model chain
    /// pair that actually answered.
    pub async fn ask_with_usage(
        &self,
        request: &str,
    ) -> Result<(String, crate::Usage), AgentError> {
        let chain = ModelChain::from_pairs(self.model_chain())
            .ok_or_else(|| AgentError::Provider("the Model chain is empty".into()))?;
        let role = &self.state.role;
        let mut node = AgentNode::with_chain(chain, &role.instructions);
        if let Some(listener) = mutex(&self.state.on_event).clone() {
            node = node.on_event(move |e| listener(e));
        }
        for tool in &role.tools {
            node = node.tool(tool.clone());
        }
        let mut conversation = self.state.conversation.lock().await;
        let (answer, turns, usage) = node.converse(&conversation, request).await?;
        conversation.extend(turns);
        prune_history(&mut conversation);
        *mutex(&self.state.history_len) = conversation.len();
        Ok((answer, usage))
    }
}

/// Builds the git branch name `<agent-name>/<topic>`; the topic is lowercased and
/// reduced to letters, digits and dashes.
pub fn branch_name(agent_name: &str, topic: &str) -> String {
    let mut slug = String::new();
    for c in topic.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    format!("{agent_name}/{}", slug.trim_end_matches('-'))
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds a Node run by a named [`Agent`], so its conversation carries over from
    /// one Job to the next. `read` takes the request from the State, and `write`
    /// puts the answer (or its failure) back.
    pub fn named_agent_node<R, W>(self, name: &str, agent: Agent, read: R, write: W) -> Self
    where
        R: Fn(&S) -> String + Send + Sync + 'static,
        W: Fn(S, Result<String, AgentError>) -> S + Send + Sync + 'static,
    {
        let (read, write) = (Arc::new(read), Arc::new(write));
        self.node(name, move |state: S| {
            let (agent, read, write) = (agent.clone(), read.clone(), write.clone());
            async move {
                let answer = agent.ask(&read(&state)).await;
                write(state, answer)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> Message {
        Message::new_human_message(text)
    }

    #[test]
    fn history_is_pruned_to_the_cap_keeping_the_newest_turns() {
        let mut history: Vec<Message> = (0..MAX_HISTORY_MESSAGES + 3)
            .map(|i| message(&format!("turn {i}")))
            .collect();
        prune_history(&mut history);
        assert_eq!(history.len(), MAX_HISTORY_MESSAGES);
        // The 100 kept are the newest: "turn 0".."turn 2" were dropped.
        assert_eq!(history.first().unwrap().content, "turn 3");
        assert_eq!(history.last().unwrap().content, "turn 102");
    }

    #[test]
    fn history_at_or_below_the_cap_is_untouched() {
        let mut history: Vec<Message> = (0..10)
            .map(|i| message(&format!("turn {i}")))
            .collect();
        prune_history(&mut history);
        assert_eq!(history.len(), 10);
        let mut empty: Vec<Message> = Vec::new();
        prune_history(&mut empty);
        assert!(empty.is_empty());
    }
}
