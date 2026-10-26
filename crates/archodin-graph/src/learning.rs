//! Learning from experience, and the Akasha (ADR 0004).
//!
//! A [`Learner`] runs an Agent and, after a successful run with at least
//! [`MIN_TOOL_CALLS`] Tool calls, or after a person corrects the Agent, has a
//! writer model draft a new Skill or a refinement of one the Role already has.
//! The draft goes straight to Codex's [`SkillLibrary`] as a pending Skill, so a
//! person still approves it. The [`Akasha`] holds the facts a Role's Agents
//! share, deduplicated so repeating a fact costs nothing.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Agent, AgentError, AgentNode, AgentTool, Provider, Skill, SkillError, SkillLibrary};
use crate::{SkillKind, TypedTool};

/// How many Tool calls make a run worth writing down as a Skill.
pub const MIN_TOOL_CALLS: usize = 5;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

tokio::task_local! {
    static RUN_CALLS: Arc<AtomicUsize>;
}

/// Why learning failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LearnError {
    /// The Agent itself failed.
    #[error(transparent)]
    Agent(#[from] AgentError),
    /// The writer's draft could not be read as a Skill.
    #[error(transparent)]
    Skill(#[from] SkillError),
    /// The writer model failed.
    #[error("the writer failed: {0}")]
    Writer(String),
}

/// What one learning run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Learned {
    /// The Agent's answer.
    pub answer: String,
    /// How many counted Tool calls the run made.
    pub tool_calls: usize,
    /// The name of the Skill sent to the approval queue, if one was.
    pub proposed: Option<String>,
}

/// Runs Agents and turns experience into proposed Skills.
#[derive(Clone)]
pub struct Learner {
    library: SkillLibrary,
    writer: Provider,
    min_tool_calls: usize,
    last_request: Arc<Mutex<HashMap<String, String>>>,
}

struct Counted<T>(T);

impl<T: TypedTool> TypedTool for Counted<T> {
    type Input = T::Input;
    type Error = T::Error;
    fn name(&self) -> String {
        self.0.name()
    }
    fn description(&self) -> String {
        self.0.description()
    }
    async fn call(&self, input: Self::Input) -> Result<String, Self::Error> {
        let _ = RUN_CALLS.try_with(|c| c.fetch_add(1, Ordering::SeqCst));
        self.0.call(input).await
    }
}

const WRITER_INSTRUCTIONS: &str =
    "You write Skills for an AI employee in agentskills.io SKILL.md format. \
Reply with the SKILL.md only: front matter between `---` lines with `name`, `description` and \
`kind` (`flow` or `discipline`), then the instructions.";

impl Learner {
    /// A Learner whose proposals go to `library`, drafted by the model behind `writer`.
    pub fn new(library: SkillLibrary, writer: Provider) -> Self {
        Self {
            library,
            writer,
            min_tool_calls: MIN_TOOL_CALLS,
            last_request: Arc::default(),
        }
    }

    /// Changes how many Tool calls make a run worth learning from (default 5).
    pub fn min_tool_calls(mut self, n: usize) -> Self {
        self.min_tool_calls = n;
        self
    }

    /// Wraps a Tool so the Learner counts its calls; give the result to a Role.
    pub fn counting<T: TypedTool>(&self, tool: T) -> AgentTool {
        AgentTool::typed(Counted(tool))
    }

    /// Runs `agent` on `request`; after a run with enough Tool calls, proposes a Skill.
    pub async fn ask(&self, agent: &Agent, request: &str) -> Result<Learned, LearnError> {
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = RUN_CALLS.scope(calls.clone(), agent.ask(request)).await?;
        lock(&self.last_request).insert(agent.name().into(), request.into());
        let tool_calls = calls.load(Ordering::SeqCst);
        let proposed = if tool_calls >= self.min_tool_calls {
            let prompt = format!(
                "An Agent of Role `{role}` just finished a task using {tool_calls} Tool calls.\n\
                 Task: {request}\nResult: {answer}\n\
                 {existing}\nWrite a Skill that captures the procedure so the next run is easier. \
                 If one of the existing Skills covers it, refine that one under the same name.",
                role = agent.role(),
                existing = self.existing(agent.role()),
            );
            Some(self.propose(agent.role(), &prompt).await?)
        } else {
            None
        };
        Ok(Learned {
            answer,
            tool_calls,
            proposed,
        })
    }

    /// Passes a person's `correction` to `agent` and proposes a refinement of the
    /// Role's Skill (or a new one) that includes the lesson.
    pub async fn correct(&self, agent: &Agent, correction: &str) -> Result<Learned, LearnError> {
        let calls = Arc::new(AtomicUsize::new(0));
        let answer = RUN_CALLS
            .scope(calls.clone(), agent.ask(correction))
            .await?;
        let earlier = lock(&self.last_request)
            .get(agent.name())
            .cloned()
            .unwrap_or_default();
        let prompt = format!(
            "An Agent of Role `{role}` was corrected by a person.\n\
             Earlier task: {earlier}\nCorrection: {correction}\nAgent's reply: {answer}\n\
             {existing}\nRefine the relevant existing Skill under the same name so it includes \
             this lesson, or write a new Skill if none applies.",
            role = agent.role(),
            existing = self.existing(agent.role()),
        );
        let proposed = Some(self.propose(agent.role(), &prompt).await?);
        Ok(Learned {
            answer,
            tool_calls: calls.load(Ordering::SeqCst),
            proposed,
        })
    }

    fn existing(&self, role: &str) -> String {
        let skills = self.library.skills_for_role(role);
        if skills.is_empty() {
            return "The Role has no Skills yet.\n".into();
        }
        let mut out = String::from("The Role's existing Skills:\n");
        for s in skills {
            let kind = match s.kind() {
                SkillKind::Flow => "flow",
                SkillKind::Discipline => "discipline",
            };
            out.push_str(&format!(
                "---\nname: {}\ndescription: {}\nkind: {kind}\n---\n{}\n",
                s.name(),
                s.description(),
                s.body()
            ));
        }
        out
    }

    async fn propose(&self, role: &str, prompt: &str) -> Result<String, LearnError> {
        let writer = AgentNode::new(self.writer.clone(), WRITER_INSTRUCTIONS);
        let reply = writer
            .answer(prompt)
            .await
            .map_err(|e| LearnError::Writer(e.to_string()))?;
        let skill = Skill::from_markdown(strip_fence(&reply))?;
        let name = skill.name().to_string();
        self.library.propose(role, skill);
        Ok(name)
    }
}

/// Removes a surrounding Markdown code fence a model may wrap its reply in.
fn strip_fence(reply: &str) -> &str {
    let t = reply.trim();
    match t.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.split_once('\n').map_or("", |(_, r)| r);
            rest.strip_suffix("```").unwrap_or(rest).trim_start()
        }
        None => t,
    }
}

/// The facts a Role's Agents share, kept free of duplicates.
#[derive(Clone, Default)]
pub struct Akasha {
    facts: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
}

/// The key two facts share when the Akasha treats them as the same fact:
/// spacing, case and closing punctuation are ignored.
pub fn fact_key(fact: &str) -> String {
    fact.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '!', '?', ';', ':'])
        .to_lowercase()
}

#[derive(Deserialize, JsonSchema)]
struct RememberInput {
    /// One short, self-contained fact about a client, project or decision.
    fact: String,
}

struct Remember {
    akasha: Akasha,
    role: String,
}

impl TypedTool for Remember {
    type Input = RememberInput;
    type Error = std::convert::Infallible;
    fn name(&self) -> String {
        "remember".into()
    }
    fn description(&self) -> String {
        "Saves a fact about a client, project or decision to the Akasha every Agent of your Role reads"
            .into()
    }
    async fn call(&self, input: RememberInput) -> Result<String, Self::Error> {
        let added = self.akasha.remember_for_role(&self.role, &input.fact);
        Ok(if added { "remembered" } else { "already known" }.into())
    }
}

impl Akasha {
    /// An empty Akasha.
    pub fn new() -> Self {
        Self::default()
    }

    /// Saves `fact` for the Role of `agent`; false when it was already known.
    pub fn remember(&self, agent: &Agent, fact: &str) -> bool {
        self.remember_for_role(agent.role(), fact)
    }

    /// Saves `fact` for `role`; false when the Role already has it (ignoring
    /// case, spacing and closing punctuation) or the fact is empty.
    pub fn remember_for_role(&self, role: &str, fact: &str) -> bool {
        let fact = fact.split_whitespace().collect::<Vec<_>>().join(" ");
        let k = fact_key(&fact);
        if k.is_empty() {
            return false;
        }
        let mut all = lock(&self.facts);
        let list = all.entry(role.to_string()).or_default();
        if list.iter().any(|f| fact_key(f) == k) {
            return false;
        }
        list.push(fact);
        true
    }

    /// The Roles that have at least one fact.
    pub fn roles(&self) -> Vec<String> {
        lock(&self.facts).keys().cloned().collect()
    }

    /// The facts of `role`, oldest first.
    pub fn facts(&self, role: &str) -> Vec<String> {
        lock(&self.facts).get(role).cloned().unwrap_or_default()
    }

    /// The facts every Agent of `agent`'s Role shares.
    pub fn facts_for_agent(&self, agent: &Agent) -> Vec<String> {
        self.facts(agent.role())
    }

    /// The Role's facts as a bullet list, ready to put in an Agent's instructions.
    pub fn briefing(&self, role: &str) -> String {
        self.facts(role)
            .iter()
            .map(|f| format!("- {f}\n"))
            .collect()
    }

    /// A Tool called `remember` that lets Agents of `role` save facts themselves.
    pub fn remember_tool(&self, role: &str) -> AgentTool {
        AgentTool::typed(Remember {
            akasha: self.clone(),
            role: role.into(),
        })
    }
}
