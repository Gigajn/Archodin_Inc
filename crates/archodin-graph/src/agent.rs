//! The Agent as a Node: an LLM with instructions and Tools, reached through an
//! OpenAI-compatible Provider.
//!
//! An [`AgentNode`] keeps calling Tools until the model produces an answer. Tool
//! calls the model asks for in one step are independent, so they run concurrently.
//! Tools are typed ([`TypedTool`]) or adapted from langchain-rust
//! ([`AgentTool::from_langchain`]).

use std::future::Future;
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use langchain_rust::language_models::llm::LLM;
use langchain_rust::language_models::options::CallOptions;
use langchain_rust::llm::openai::{OpenAI, OpenAIConfig};
use langchain_rust::schemas::{FunctionDefinition, Message};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::GraphBuilder;

/// The most model round trips one Agent run may take before it gives up.
const MAX_ROUNDS: usize = 10;

/// An OpenAI-compatible Provider account and model: 9router, OpenRouter, Groq,
/// LM Studio and vLLM are all reached by base URL, API key and model name.
#[derive(Debug, Clone)]
pub struct Provider {
    base_url: String,
    api_key: String,
    model: String,
}

impl Provider {
    /// `base_url` is the API root, for example `http://localhost:1234/v1`.
    pub fn openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    fn llm(&self, tools: &[AgentTool]) -> OpenAI<OpenAIConfig> {
        let config = OpenAIConfig::new()
            .with_api_base(&self.base_url)
            .with_api_key(&self.api_key);
        let mut options = CallOptions::default();
        if !tools.is_empty() {
            options = options.with_functions(
                tools
                    .iter()
                    .map(|t| FunctionDefinition::new(&t.name, &t.description, t.parameters.clone()))
                    .collect(),
            );
        }
        OpenAI::new(config)
            .with_model(self.model.clone())
            .with_options(options)
    }
}

/// A Tool with a structured input: the schema the model sees is generated from
/// [`Input`](TypedTool::Input), and errors are `Send + Sync`.
pub trait TypedTool: Send + Sync + 'static {
    /// The structured input the model fills in.
    type Input: DeserializeOwned + JsonSchema + Send;
    /// The error a call can fail with; the Agent shows it to the model.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The name the model calls the Tool by.
    fn name(&self) -> String;
    /// What the Tool does and when to use it.
    fn description(&self) -> String;
    /// Runs the Tool and returns the result the Agent reads.
    fn call(&self, input: Self::Input) -> impl Future<Output = Result<String, Self::Error>> + Send;
}

type Runner = Arc<dyn Fn(String) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

/// A Tool an Agent can call, whatever its origin.
#[derive(Clone)]
pub struct AgentTool {
    name: String,
    description: String,
    parameters: Value,
    runner: Runner,
}

impl AgentTool {
    /// Wraps a [`TypedTool`], generating its parameter schema from the input type.
    pub fn typed<T: TypedTool>(tool: T) -> Self {
        let tool = Arc::new(tool);
        let mut parameters = serde_json::to_value(schemars::schema_for!(T::Input))
            .expect("a schema is always valid JSON");
        if let Some(object) = parameters.as_object_mut() {
            object.remove("$schema");
        }
        Self {
            name: tool.name(),
            description: tool.description(),
            parameters,
            runner: Arc::new(move |arguments| {
                let tool = tool.clone();
                Box::pin(async move {
                    let input: T::Input = serde_json::from_str(&arguments)
                        .map_err(|e| format!("invalid arguments: {e}"))?;
                    tool.call(input).await.map_err(|e| e.to_string())
                })
            }),
        }
    }

    /// Adapts an existing langchain-rust Tool.
    pub fn from_langchain(tool: Arc<dyn langchain_rust::tools::Tool>) -> Self {
        Self {
            name: tool.name(),
            description: tool.description(),
            parameters: tool.parameters(),
            runner: Arc::new(move |arguments| {
                let tool = tool.clone();
                Box::pin(async move { tool.call(&arguments).await.map_err(|e| e.to_string()) })
            }),
        }
    }

    /// The name the model calls the Tool by.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The JSON schema of the Tool's input.
    pub fn parameters(&self) -> &Value {
        &self.parameters
    }
}

/// Why an Agent run produced no answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    /// The Provider call failed.
    #[error("provider error: {0}")]
    Provider(String),
    /// The model kept asking for Tools without answering.
    #[error("the agent did not answer within {0} rounds")]
    TooManyRounds(usize),
}

/// An Agent that can run as a Node: instructions, Tools and a Provider.
#[derive(Clone)]
pub struct AgentNode {
    provider: Provider,
    instructions: String,
    tools: Vec<AgentTool>,
}

impl AgentNode {
    /// An Agent with `instructions` that talks to `provider`.
    pub fn new(provider: Provider, instructions: impl Into<String>) -> Self {
        Self {
            provider,
            instructions: instructions.into(),
            tools: Vec::new(),
        }
    }

    /// Gives the Agent a Tool.
    pub fn tool(mut self, tool: AgentTool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Answers `request`, calling Tools as the model asks.
    pub async fn answer(&self, request: &str) -> Result<String, AgentError> {
        let llm = self.provider.llm(&self.tools);
        let mut messages = vec![
            Message::new_system_message(&self.instructions),
            Message::new_human_message(request),
        ];
        for _ in 0..MAX_ROUNDS {
            let result = llm
                .generate(&messages)
                .await
                .map_err(|e| AgentError::Provider(e.to_string()))?;
            if result.tool_calls.is_empty() {
                return Ok(result.generation);
            }
            messages.push(Message::new_ai_message_with_tool_calls(
                &result.generation,
                &result.tool_calls,
            ));
            let outputs = join_all(result.tool_calls.iter().map(|call| async move {
                match self.tools.iter().find(|t| t.name == call.name) {
                    Some(tool) => (tool.runner)(call.arguments.clone())
                        .await
                        .unwrap_or_else(|e| format!("error: {e}")),
                    None => format!("error: unknown tool `{}`", call.name),
                }
            }))
            .await;
            for (call, output) in result.tool_calls.iter().zip(outputs) {
                messages.push(Message::new_tool_message(output, &call.id));
            }
        }
        Err(AgentError::TooManyRounds(MAX_ROUNDS))
    }
}

impl<S: Send + 'static> GraphBuilder<S> {
    /// Adds an Agent Node: `read` takes the request from the State, and `write`
    /// puts the Agent's answer (or its failure) back into it.
    pub fn agent_node<R, W>(self, name: &str, agent: AgentNode, read: R, write: W) -> Self
    where
        R: Fn(&S) -> String + Send + Sync + 'static,
        W: Fn(S, Result<String, AgentError>) -> S + Send + Sync + 'static,
    {
        let agent = Arc::new(agent);
        let read = Arc::new(read);
        let write = Arc::new(write);
        self.node(name, move |state: S| {
            let (agent, read, write) = (agent.clone(), read.clone(), write.clone());
            async move {
                let answer = agent.answer(&read(&state)).await;
                write(state, answer)
            }
        })
    }
}
