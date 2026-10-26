//! Providers with their own API instead of the OpenAI-compatible one: Gemini,
//! and langchain-rust's Claude, Ollama and Bedrock clients, so any of them can be a
//! pair in a Model chain next to OpenAI-compatible ones.

use std::sync::Arc;

use langchain_rust::language_models::llm::LLM;
use langchain_rust::llm::Claude;

use crate::agent::{KeySource, Provider};
use crate::gemini::Gemini;

/// Builds the LLM of a native Provider from its API key.
#[derive(Clone)]
pub(crate) struct Native(Arc<dyn Fn(String) -> Box<dyn LLM> + Send + Sync>);

impl Native {
    fn new(build: impl Fn(String) -> Box<dyn LLM> + Send + Sync + 'static) -> Self {
        Self(Arc::new(build))
    }

    pub(crate) fn build(&self, key: String) -> Box<dyn LLM> {
        (self.0)(key)
    }
}

impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Native(..)")
    }
}

const GEMINI_URL: &str = "https://generativelanguage.googleapis.com";
const CLAUDE_URL: &str = "https://api.anthropic.com";

impl Provider {
    /// A Gemini Provider (generateContent chat, function calling, streaming).
    pub fn gemini(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::gemini_at(GEMINI_URL, api_key, model)
    }

    /// Like [`Provider::gemini`], against another API root (a proxy or a fake server).
    pub fn gemini_at(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let key = KeySource::Fixed(api_key.into());
        Self::gemini_keyed(base_url.into(), key, model.into())
    }

    /// A Gemini Provider whose API key is the vault secret `secret`, looked up at
    /// every call.
    pub fn gemini_from_vault(
        vault: &crate::Vault,
        secret: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let key = KeySource::Vault(vault.clone(), secret.into());
        Self::gemini_keyed(GEMINI_URL.into(), key, model.into())
    }

    fn gemini_keyed(base_url: String, key: KeySource, model: String) -> Self {
        let (url, name) = (base_url.clone(), model.clone());
        let native = Native::new(move |key| {
            Box::new(Gemini::new(key, name.clone()).with_base_url(url.clone()))
        });
        Self::with_native(base_url, key, model, native)
    }

    /// A Claude Provider (langchain-rust's Claude client).
    pub fn claude(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::claude_at(CLAUDE_URL, api_key, model)
    }

    /// Like [`Provider::claude`], against another API root (a proxy or a fake server).
    pub fn claude_at(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let key = KeySource::Fixed(api_key.into());
        Self::claude_keyed(base_url.into(), key, model.into())
    }

    /// A Claude Provider whose API key is the vault secret `secret`.
    pub fn claude_from_vault(
        vault: &crate::Vault,
        secret: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let key = KeySource::Vault(vault.clone(), secret.into());
        Self::claude_keyed(CLAUDE_URL.into(), key, model.into())
    }

    fn claude_keyed(base_url: String, key: KeySource, model: String) -> Self {
        let (url, name) = (base_url.clone(), model.clone());
        let native = Native::new(move |key| {
            Box::new(
                Claude::new()
                    .with_api_key(key)
                    .with_model(name.clone())
                    .with_base_url(url.clone()),
            )
        });
        Self::with_native(base_url, key, model, native)
    }

    /// A Provider backed by any langchain-rust LLM (Bedrock, DeepSeek, Qwen, ...).
    /// `account` names it in [`ChainEvent`](crate::ChainEvent)s. Tools are passed to
    /// the LLM through [`LLM::add_options`], as langchain-rust's own agents do.
    pub fn from_llm(
        account: impl Into<String>,
        model: impl Into<String>,
        llm: impl LLM + Clone + 'static,
    ) -> Self {
        let account = account.into();
        let native = Native::new(move |_| llm.clone_box());
        let key = KeySource::Fixed(String::new());
        Self::with_native(account.clone(), key, model.into(), native).account(account)
    }

    /// An Ollama Provider reached at `base_url`, for example `http://localhost:11434`.
    ///
    /// # Panics
    /// When `base_url` is not a valid URL.
    #[cfg(feature = "ollama")]
    pub fn ollama(base_url: impl AsRef<str>, model: impl Into<String>) -> Self {
        use langchain_rust::llm::ollama::client::{Ollama, OllamaClient};
        let model = model.into();
        let client =
            Arc::new(OllamaClient::try_new(base_url.as_ref()).expect("a valid Ollama base URL"));
        let llm = Ollama::new(client, model.clone(), None);
        Self::from_llm(base_url.as_ref(), model, llm)
    }

    /// A Bedrock Provider for the model set in `llm`'s configuration.
    #[cfg(feature = "bedrock")]
    pub fn bedrock(llm: langchain_rust::llm::bedrock::Bedrock) -> Self {
        let model = llm.config().model.model_id().to_string();
        Self::from_llm("bedrock", model, llm)
    }
}
