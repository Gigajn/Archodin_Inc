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
use serde_json::{json, Value};

use crate::chain::{ChainEvent, ModelChain, ModelPair};
use crate::http_guard::validate_endpoint;
use crate::registry::{ModelError, ModelKind};
use crate::GraphBuilder;

/// The most model round trips one Agent run may take before it gives up.
const MAX_ROUNDS: usize = 10;

/// An OpenAI-compatible Provider account and model: 9router, OpenRouter, Groq,
/// LM Studio and vLLM are all reached by base URL, API key and model name.
///
/// NOT serializable — deliberately (ADR 0025 §4). A `Provider` holds a key
/// (`KeySource`), so it must never enter a Store, a Checkpoint, Job state,
/// a Briefing or an Akasha: only the account/model NAME travels there, and a
/// fresh Provider is built from the Registry + Vault when one is needed.
/// Do NOT add `Serialize`/`Deserialize` to this struct.
#[derive(Debug, Clone)]
pub struct Provider {
    base_url: String,
    api_key: KeySource,
    model: String,
    account: Option<String>,
    pricing: Option<crate::Pricing>,
    /// Set for Providers with their own API (see `native.rs`).
    native: Option<crate::native::Native>,
    /// The Registry kind, when the constructor knew it; `None` for a native
    /// Provider whose constructor has not recorded one yet.
    kind: Option<ModelKind>,
    /// Whether a call needs a key at all: false only for Ollama, which has
    /// no auth. Every other kind refuses an empty key in [`Provider::validate`].
    key_required: bool,
}

impl Provider {
    /// `base_url` is the API root, for example `http://localhost:1234/v1`.
    ///
    /// This constructor does NOT validate: an empty or unparsable base URL,
    /// an empty model, or an empty key all build fine here, exactly as
    /// before. Validation lives in [`Provider::try_openai_compatible`] and
    /// in [`Provider::validate`], and the Registry calls `validate` when an
    /// account is registered — so an invalid Provider never serves a Job
    /// through the Registry path.
    pub fn openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: KeySource::Fixed(api_key.into()),
            model: model.into(),
            account: None,
            pricing: None,
            native: None,
            kind: Some(ModelKind::OpenAiCompatible),
            key_required: true,
        }
    }

    /// Like [`Provider::openai_compatible`], but refuses a Provider that
    /// could not serve a call: an empty or invalid base URL, an empty model
    /// name, or an empty key. The error names the kind, account and model —
    /// never the key.
    pub fn try_openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let provider = Self::openai_compatible(base_url, api_key, model);
        provider.validate()?;
        Ok(provider)
    }

    /// Shape validation, no network: the base URL must pass
    /// `http_guard::validate_endpoint`, the model name must not be empty,
    /// and — unless this Provider needs no key (Ollama) — the key must not
    /// be empty (a `Fixed("")` key, or a Vault secret with an empty name).
    /// A Vault key is checked by NAME only: whether the Vault actually holds
    /// it is a runtime lookup, not a shape question.
    pub fn validate(&self) -> Result<(), ModelError> {
        // The kind is only a label in the error; a Provider built before its
        // kind was recorded reports as `openai_compatible` rather than
        // failing validation for want of a label.
        let kind = self.kind.unwrap_or(ModelKind::OpenAiCompatible);
        let account = self
            .account
            .clone()
            .unwrap_or_else(|| self.base_url.clone());
        if let Err(e) = validate_endpoint(&self.base_url) {
            return Err(ModelError::invalid(
                kind,
                &account,
                &self.model,
                format!("base URL refused: {e}"),
            ));
        }
        if self.model.is_empty() {
            return Err(ModelError::invalid(
                kind,
                &account,
                &self.model,
                "the model name must not be empty",
            ));
        }
        if self.key_required {
            let key_missing = match &self.api_key {
                KeySource::Fixed(key) => key.is_empty(),
                KeySource::Vault(_, name) => name.is_empty(),
            };
            if key_missing {
                return Err(ModelError::key_missing(kind, &account, &self.model));
            }
        }
        Ok(())
    }

    /// Records the Registry kind of a Provider built through `with_native`
    /// (Gemini, Claude, Ollama), so `validate` and error mapping name it.
    pub(crate) fn set_kind(mut self, kind: ModelKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Marks a Provider as needing no key (Ollama only): `validate` then
    /// accepts an empty key. Only the Ollama constructor calls this, so it
    /// exists only when the `ollama` feature is on.
    #[cfg(feature = "ollama")]
    pub(crate) fn no_key_required(mut self) -> Self {
        self.key_required = false;
        self
    }

    /// A Provider reached through its own API: `build` makes the LLM from the API key.
    pub(crate) fn with_native(
        base_url: String,
        api_key: KeySource,
        model: String,
        native: crate::native::Native,
    ) -> Self {
        Self {
            base_url,
            api_key,
            model,
            account: None,
            pricing: None,
            native: Some(native),
            // The kind is recorded by the native constructor that knows it
            // (`set_kind`); a constructor that never calls it leaves `None`
            // and `validate` falls back to the `openai_compatible` label.
            kind: None,
            key_required: true,
        }
    }

    /// A Provider account whose API key is the vault secret `secret`, looked up at
    /// every call so a rotated key applies without a restart.
    pub fn from_vault(
        base_url: impl Into<String>,
        vault: &crate::Vault,
        secret: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let mut provider = Self::openai_compatible(base_url, "", model);
        provider.api_key = KeySource::Vault(vault.clone(), secret.into());
        provider
    }

    /// Sets what this Provider account charges, so usage answered by it has a cost.
    pub fn pricing(mut self, pricing: crate::Pricing) -> Self {
        self.pricing = Some(pricing);
        self
    }

    pub(crate) fn priced(&self, usage: crate::Usage) -> crate::Usage {
        self.pricing.map_or(usage, |p| p.price(usage))
    }

    /// Names this Provider account (for example `work`), so events can tell two
    /// accounts of the same Provider apart. Defaults to the base URL.
    pub fn account(mut self, name: impl Into<String>) -> Self {
        self.account = Some(name.into());
        self
    }

    pub(crate) fn pair(&self) -> ModelPair {
        ModelPair::new(
            self.account
                .clone()
                .unwrap_or_else(|| self.base_url.clone()),
            self.model.clone(),
        )
    }

    /// Hides this Provider's API key from error text, whatever its source:
    /// a Vault secret is scrubbed by the vault, and a key set directly is
    /// scrubbed here, so neither can leak into an error, an event or a log
    /// (KR-06/SD-08). An empty key redacts nothing.
    pub(crate) fn redact(&self, text: String) -> String {
        match &self.api_key {
            KeySource::Vault(vault, _) => vault.redact(&text),
            KeySource::Fixed(key) => {
                if key.is_empty() {
                    text
                } else {
                    text.replace(key.as_str(), "[redacted]")
                }
            }
        }
    }

    pub(crate) fn llm(&self, tools: &[AgentTool]) -> Result<Box<dyn LLM>, String> {
        let key = match &self.api_key {
            KeySource::Fixed(key) => key.clone(),
            KeySource::Vault(vault, name) => vault
                .resolve(name)
                .map_err(|e| e.to_string())?
                .expose()
                .to_string(),
        };
        if let Some(native) = &self.native {
            let mut llm = native.build(key);
            if !tools.is_empty() {
                llm.add_options(CallOptions::new().with_functions(definitions(tools)));
            }
            return Ok(llm);
        }
        let config = OpenAIConfig::new()
            .with_api_base(&self.base_url)
            .with_api_key(key);
        // Every OpenAI-compatible call goes through the guarded client
        // (ADR 0025 §3): 30 s timeout, 2 MiB cap, http-acl SSRF filtering.
        // A client that cannot be built is an error, never a silent
        // fallback to an unguarded client — and the message names no key.
        let guarded = crate::http_guard::shared_client()
            .map_err(|e| format!("could not build the guarded HTTP client: {e}"))?;
        let mut options = CallOptions::default();
        if !tools.is_empty() {
            options = options.with_functions(definitions(tools));
        }
        Ok(Box::new(
            OpenAI::new(config)
                .with_http_client(guarded)
                .with_model(self.model.clone())
                .with_options(options),
        ))
    }
}

fn definitions(tools: &[AgentTool]) -> Vec<FunctionDefinition> {
    tools
        .iter()
        .map(|t| FunctionDefinition::new(&t.name, &t.description, t.parameters.clone()))
        .collect()
}

/// Where a Provider's key comes from: held directly (`Fixed`, test and
/// short-lived bootstrap only) or looked up in the Vault by name at every
/// call (`Vault`, so rotation applies without a restart).
///
/// NOT serializable — deliberately (ADR 0025 §4), like [`Provider`]: a
/// `KeySource` carries or names a key, so it must never enter a Store, a
/// Checkpoint, Job state, a Briefing or an Akasha. Do NOT add
/// `Serialize`/`Deserialize` to this enum. Its `Debug` never prints a value.
#[derive(Clone)]
pub(crate) enum KeySource {
    Fixed(String),
    Vault(crate::Vault, String),
}

impl std::fmt::Debug for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(_) => f.write_str("KeySource::Fixed(..)"),
            Self::Vault(_, name) => write!(f, "KeySource::Vault({name})"),
        }
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
        // A schema generated from a `JsonSchema` type always converts to JSON;
        // the fallback keeps the tool usable with an empty object schema
        // instead of panicking the job if that ever stops holding.
        let mut parameters = serde_json::to_value(schemars::schema_for!(T::Input))
            .unwrap_or_else(|_| json!({"type": "object"}));
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

    /// Builds a Tool whose schema is only known at run time (for example one an
    /// MCP server lists); `call` receives the arguments as a JSON string.
    pub fn dynamic<F>(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        call: F,
    ) -> Self
    where
        F: Fn(String) -> BoxFuture<'static, Result<String, String>> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            runner: Arc::new(call),
        }
    }

    /// Calls the Tool with `arguments` (a JSON string), as an Agent would.
    pub async fn call(&self, arguments: &str) -> Result<String, String> {
        (self.runner)(arguments.to_string()).await
    }

    /// What the Tool does and when to use it.
    pub fn description(&self) -> &str {
        &self.description
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

impl From<ModelError> for AgentError {
    /// The boundary mapping (ADR 0023 §4): a typed model error becomes the
    /// Provider error an Agent reports. This is safe by construction —
    /// `ModelError`'s `Display` is built only from kind, account and model
    /// names, never from a key value or a raw upstream body — so no secret
    /// can cross here. The chain/fallback behaviour is unchanged: this only
    /// gives typed errors a typed way in.
    fn from(error: ModelError) -> Self {
        AgentError::Provider(error.to_string())
    }
}

/// An Agent that can run as a Node: instructions, Tools and a Provider.
#[derive(Clone)]
pub struct AgentNode {
    chain: ModelChain,
    instructions: String,
    tools: Vec<AgentTool>,
    on_event: Option<Arc<dyn Fn(ChainEvent) + Send + Sync>>,
}

impl AgentNode {
    /// An Agent with `instructions` that talks to `provider`.
    pub fn new(provider: Provider, instructions: impl Into<String>) -> Self {
        Self::with_chain(ModelChain::new(provider), instructions)
    }

    /// An Agent with `instructions` that follows a Model chain (ADR 0005).
    pub fn with_chain(chain: ModelChain, instructions: impl Into<String>) -> Self {
        Self {
            chain,
            instructions: instructions.into(),
            tools: Vec::new(),
            on_event: None,
        }
    }

    /// Reports each fallback along the Model chain to `listener`.
    pub fn on_event(mut self, listener: impl Fn(ChainEvent) + Send + Sync + 'static) -> Self {
        self.on_event = Some(Arc::new(listener));
        self
    }

    /// Gives the Agent a Tool.
    pub fn tool(mut self, tool: AgentTool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Answers `request`, calling Tools as the model asks.
    pub async fn answer(&self, request: &str) -> Result<String, AgentError> {
        self.answer_with_usage(request).await.map(|(a, _)| a)
    }

    /// Like [`AgentNode::answer`], also returning the tokens all model round trips
    /// used, priced by the Model chain pair that answered each (see
    /// [`Provider::pricing`]); the cost is 0 for a pair without pricing.
    pub async fn answer_with_usage(
        &self,
        request: &str,
    ) -> Result<(String, crate::Usage), AgentError> {
        self.converse(&[], request)
            .await
            .map(|(answer, _, usage)| (answer, usage))
    }

    /// Answers `request` after the earlier turns in `history`, and returns the
    /// answer with the turns this call added (the request and the answer).
    pub(crate) async fn converse(
        &self,
        history: &[Message],
        request: &str,
    ) -> Result<(String, Vec<Message>, crate::Usage), AgentError> {
        let mut current = 0;
        let mut usage = crate::Usage::default();
        let mut messages = vec![Message::new_system_message(&self.instructions)];
        messages.extend_from_slice(history);
        messages.push(Message::new_human_message(request));
        for _ in 0..MAX_ROUNDS {
            let result = self
                .chain
                .generate(&mut current, &self.tools, &messages, self.on_event.as_ref())
                .await?;
            if let Some(t) = &result.tokens {
                let used = crate::Usage::new(
                    u64::from(t.prompt_tokens),
                    u64::from(t.completion_tokens),
                    0.0,
                );
                let priced = self
                    .chain
                    .provider(current)
                    .map(|p| p.priced(used))
                    .unwrap_or(used);
                usage.add(priced);
            }
            if result.tool_calls.is_empty() {
                let turns = vec![
                    Message::new_human_message(request),
                    Message::new_ai_message(&result.generation),
                ];
                return Ok((result.generation, turns, usage));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_api_key_is_redacted_from_error_text() {
        let provider =
            Provider::openai_compatible("http://localhost:9/v1", "sk-fixed-secret", "m");
        assert_eq!(
            provider.redact("provider error: bad key sk-fixed-secret".to_string()),
            "provider error: bad key [redacted]"
        );
        assert_eq!(
            provider.redact("nothing secret here".to_string()),
            "nothing secret here"
        );
    }

    #[test]
    fn an_empty_fixed_key_redacts_nothing() {
        let provider = Provider::openai_compatible("http://localhost:9/v1", "", "m");
        assert_eq!(provider.redact("plain text".to_string()), "plain text");
    }

    #[test]
    fn a_vault_key_is_redacted_from_error_text() {
        let vault = crate::Vault::new([7; 32].into());
        vault.put("work", "sk-vault-secret");
        let provider = Provider::from_vault("http://localhost:9/v1", &vault, "work", "m");
        assert_eq!(
            provider.redact("bad key sk-vault-secret".to_string()),
            "bad key [redacted]"
        );
    }

    #[test]
    fn try_openai_compatible_accepts_a_well_shaped_provider() {
        let provider =
            Provider::try_openai_compatible("http://localhost:9/v1", "sk-key", "m").unwrap();
        assert!(provider.validate().is_ok());
        // A Vault-backed Provider validates by secret NAME, not by whether
        // the Vault holds it — that is a runtime lookup, not shape.
        let vault = crate::Vault::new([7; 32].into());
        assert!(
            Provider::from_vault("http://localhost:9/v1", &vault, "work", "m")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn try_openai_compatible_refuses_bad_shapes() {
        assert!(matches!(
            Provider::try_openai_compatible("", "sk-key", "m").unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
        assert!(matches!(
            Provider::try_openai_compatible("not a url", "sk-key", "m").unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
        assert!(matches!(
            Provider::try_openai_compatible("http://localhost:9/v1", "sk-key", "").unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
        assert_eq!(
            Provider::try_openai_compatible("http://localhost:9/v1", "", "m").unwrap_err(),
            ModelError::key_missing(ModelKind::OpenAiCompatible, "http://localhost:9/v1", "m")
        );
    }

    #[test]
    fn the_legacy_constructor_still_builds_without_validating() {
        // Deliberate: `openai_compatible` keeps its old behaviour so
        // existing call sites keep compiling; validation is opt-in via
        // `try_openai_compatible` / `validate` and mandatory in the Registry.
        let provider = Provider::openai_compatible("", "", "");
        assert!(provider.validate().is_err());
    }

    #[test]
    fn a_model_error_becomes_the_agent_provider_error() {
        let typed = ModelError::auth_failed(ModelKind::Claude, "work", "m1");
        let agent: AgentError = typed.into();
        assert_eq!(
            agent,
            AgentError::Provider(
                "the claude provider refused the key for account `work` (model `m1`)".into()
            )
        );
    }

    /// A canary key must never surface in `Debug`, in a validation error,
    /// or in the `AgentError` it maps to (ADR 0025 §1).
    #[test]
    fn a_canary_key_never_appears_in_debug_or_error_text() {
        let canary = format!(
            "sk-canary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock runs forward")
                .as_nanos()
        );
        // A valid Provider built with the canary: its Debug hides the value.
        let provider =
            Provider::openai_compatible("http://localhost:9/v1", &canary, "m").account("work");
        assert!(!format!("{provider:?}").contains(&canary));
        // An invalid Provider built with the canary: the error names the
        // account and model, never the key.
        let err = Provider::try_openai_compatible("", &canary, "m").unwrap_err();
        assert!(!err.to_string().contains(&canary));
        assert!(!format!("{err:?}").contains(&canary));
        let agent_err: AgentError = err.into();
        assert!(!agent_err.to_string().contains(&canary));
        // And `redact` scrubs the canary out of arbitrary error text.
        assert!(!provider
            .redact(format!("provider error: bad key {canary}"))
            .contains(&canary));
    }
}
