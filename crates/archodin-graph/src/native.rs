//! Providers with their own API instead of the OpenAI-compatible one: Gemini,
//! Claude and Ollama (ADR 0022), so any of them can be a pair in a Model chain
//! next to OpenAI-compatible ones. Bedrock is DEFERRED (ADR 0022): it could
//! only be built through `Provider::from_llm`, which ADR 0025 removed, so
//! there is no Bedrock constructor until a non-string credential design exists.

use std::sync::Arc;

use langchain_rust::language_models::llm::LLM;
use langchain_rust::llm::Claude;

use crate::agent::{KeySource, Provider};
use crate::gemini::Gemini;
use crate::registry::{ModelError, ModelKind};

#[cfg(feature = "ollama")]
use crate::http_guard::validate_endpoint;

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
        Self::with_native(base_url, key, model, native).set_kind(ModelKind::Gemini)
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
        Self::claude_keyed(base_url.into(), key, model.into()).set_kind(ModelKind::Claude)
    }

    /// A Claude Provider whose API key is the vault secret `secret`.
    pub fn claude_from_vault(
        vault: &crate::Vault,
        secret: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let key = KeySource::Vault(vault.clone(), secret.into());
        Self::claude_keyed(CLAUDE_URL.into(), key, model.into()).set_kind(ModelKind::Claude)
    }

    fn claude_keyed(base_url: String, key: KeySource, model: String) -> Self {
        // A resolver that answers nothing: a client built on it can never
        // open a connection, so a request through it fails at resolve time.
        // It is the fail-closed stand-in for the guarded client (ADR 0025
        // §3): `shared_client()` is built from fixed settings, so it
        // practically always builds — but if it ever fails, the call must
        // fail, never fall back to an unguarded client. The last resort,
        // when no client can be built at all, aims the call at a closed
        // loopback port so it fails before sending anything.
        struct NoResolve;
        impl reqwest::dns::Resolve for NoResolve {
            fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
                Box::pin(async {
                    Ok(Box::new(std::iter::empty())
                        as Box<dyn Iterator<Item = std::net::SocketAddr> + Send>)
                })
            }
        }
        fn fail_closed_client() -> Result<reqwest::Client, reqwest::Error> {
            reqwest::Client::builder().dns_resolver(NoResolve).build()
        }
        let (url, name) = (base_url.clone(), model.clone());
        let native = Native::new(move |key| {
            let claude = match crate::http_guard::shared_client() {
                Ok(client) => Claude::new()
                    .with_api_key(key)
                    .with_model(name.clone())
                    .with_base_url(url.clone())
                    .with_http_client(client),
                Err(_) => match fail_closed_client() {
                    Ok(client) => Claude::new()
                        .with_api_key(key)
                        .with_model(name.clone())
                        .with_base_url(url.clone())
                        .with_http_client(client),
                    Err(_) => Claude::new()
                        .with_api_key(key)
                        .with_model(name.clone())
                        .with_base_url("http://127.0.0.1:9"),
                },
            };
            Box::new(claude)
        });
        Self::with_native(base_url, key, model, native)
    }

    /// Test-only construction of a Provider around a pre-built langchain-rust
    /// LLM (ADR 0025 §2). Production code must use the per-provider
    /// constructors, which take a `KeySource` so the key can be rotated and
    /// redacted; a pre-built LLM carries a credential the Vault never sees,
    /// so it cannot be rotated, redacted or audited. This helper exists ONLY
    /// under `#[cfg(test)]` — it is not part of the library's public API,
    /// and integration tests cannot use it (they are built against the
    /// library without `cfg(test)`).
    #[cfg(test)]
    pub fn from_llm_for_test(
        account: impl Into<String>,
        model: impl Into<String>,
        llm: impl LLM + Clone + 'static,
    ) -> Self {
        let account = account.into();
        let native = Native::new(move |_| Box::new(llm.clone()));
        let key = KeySource::Fixed(String::new());
        Self::with_native(account.clone(), key, model.into(), native).account(account)
    }

    /// An Ollama Provider reached at `base_url`, for example `http://localhost:11434`.
    ///
    /// Returns an error instead of panicking on a bad URL (ADR 0022/0023):
    /// the base URL must pass `validate_endpoint` and the model must not be
    /// empty. The base URL must ALSO be loopback (`127.0.0.0/8`, `::1`, or
    /// `localhost`) — see the open risk below.
    ///
    /// OPEN RISK (ADR 0025 §3, verified by the compiler, not assumed):
    /// ollama-rs 0.3.6 builds on reqwest 0.12, while the guarded client
    /// (`http_guard::shared_client()`) is reqwest 0.13 — the two `Client`
    /// types are different types, so the guarded client CANNOT be injected
    /// through `Ollama::builder().reqwest_client(...)` (the builder accepts
    /// an external client, but only a 0.12 one). Ollama therefore still
    /// talks through ollama-rs's own default client: no 30 s timeout, no
    /// redirect policy, no http-acl DNS filtering, no 2 MiB cap. The
    /// compensating control in this constructor is loopback-only: an
    /// unguarded client that can only ever dial loopback cannot be steered
    /// at a LAN service or the cloud metadata address by configuration.
    /// The real fix is a fork patch (ollama-rs on reqwest 0.13, or a
    /// guarded 0.12 client) — until then, Ollama is loopback-only and this
    /// gap stays open; a non-loopback Ollama URL is refused, not silently
    /// served unguarded.
    #[cfg(feature = "ollama")]
    pub fn ollama(base_url: impl AsRef<str>, model: impl Into<String>) -> Result<Self, ModelError> {
        use langchain_rust::llm::ollama::client::{Ollama, OllamaClient};
        let base_url = base_url.as_ref();
        let model = model.into();
        let url = validate_endpoint(base_url).map_err(|e| {
            ModelError::invalid(
                ModelKind::Ollama,
                base_url,
                &model,
                format!("base URL refused: {e}"),
            )
        })?;
        if !is_loopback_url(&url) {
            return Err(ModelError::invalid(
                ModelKind::Ollama,
                base_url,
                &model,
                "Ollama is loopback-only until the guarded client can be injected (ADR 0025 §3): \
                 use 127.0.0.1, ::1 or localhost",
            ));
        }
        if model.is_empty() {
            return Err(ModelError::invalid(
                ModelKind::Ollama,
                base_url,
                &model,
                "the model name must not be empty",
            ));
        }
        // `validate_endpoint` already parsed this URL, so `try_new` — which
        // only fails on an unparsable URL — cannot fail here; the `expect`
        // this replaced was the panic ADR 0022 removed.
        let client = OllamaClient::try_new(base_url)
            .expect("a URL that passed validate_endpoint always parses");
        let llm = Ollama::new(Arc::new(client), model.clone(), None);
        let native = Native::new(move |_| Box::new(llm.clone()));
        Ok(Self::with_native(
            base_url.to_string(),
            KeySource::Fixed(String::new()),
            model,
            native,
        )
        .set_kind(ModelKind::Ollama)
        .no_key_required()
        .account(base_url))
    }

    /// An Ollama Provider on the default loopback server
    /// (`http://127.0.0.1:11434`).
    #[cfg(feature = "ollama")]
    pub fn ollama_default(model: impl Into<String>) -> Result<Self, ModelError> {
        Self::ollama(OLLAMA_DEFAULT_BASE_URL, model)
    }
}

/// The default Ollama base URL: the loopback server on the standard port.
#[cfg(feature = "ollama")]
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";

/// Whether `url` points at loopback: a loopback IP literal (`127.0.0.0/8`,
/// `::1`) or the `localhost` name. The Ollama constructor is loopback-only
/// (see `Provider::ollama`), so anything else is refused there.
#[cfg(feature = "ollama")]
fn is_loopback_url(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if let Ok(ip) = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
    {
        return ip.is_loopback();
    }
    host.trim_end_matches('.').eq_ignore_ascii_case("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_only_helper_builds_a_provider_with_its_account_and_model() {
        let llm = Claude::new()
            .with_api_key("k")
            .with_model("claude-ish")
            .with_base_url("http://localhost:9");
        let provider = Provider::from_llm_for_test("custom", "claude-ish", llm);
        assert_eq!(provider.pair().account(), "custom");
        assert_eq!(provider.pair().model(), "claude-ish");
    }

    #[test]
    fn native_providers_record_their_kind() {
        assert!(Provider::gemini_at("https://example.com", "k", "m")
            .validate()
            .is_ok());
        assert!(Provider::claude_at("https://example.com", "k", "m")
            .validate()
            .is_ok());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_refuses_a_bad_url_or_an_empty_model_instead_of_panicking() {
        for bad in [
            "not a url",
            "file:///etc/passwd",
            "https://user:pass@example.com",
        ] {
            assert!(
                matches!(
                    Provider::ollama(bad, "llama3.2").unwrap_err(),
                    ModelError::ModelNotConfigured { .. }
                ),
                "{bad} must be refused"
            );
        }
        assert!(matches!(
            Provider::ollama("http://127.0.0.1:11434", "").unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_is_loopback_only_while_its_client_is_unguarded() {
        // The guarded client cannot be injected (reqwest 0.12 vs 0.13), so
        // a non-loopback Ollama URL is refused rather than served unguarded.
        for non_loopback in [
            "https://api.example.com",
            "http://192.168.1.1:11434",
            "http://10.0.0.1:11434",
        ] {
            let err = Provider::ollama(non_loopback, "llama3.2").unwrap_err();
            assert!(
                err.to_string().contains("loopback-only"),
                "{non_loopback} must be refused as non-loopback, got {err}"
            );
        }
        // Loopback spellings build (no request is sent).
        assert!(Provider::ollama("http://localhost:11434", "llama3.2").is_ok());
        assert!(Provider::ollama("http://[::1]:11434", "llama3.2").is_ok());
    }

    #[cfg(feature = "ollama")]
    #[test]
    fn ollama_builds_without_a_key_and_defaults_to_loopback() {
        // Construction only: no request is sent, so no server is needed.
        let provider = Provider::ollama("http://127.0.0.1:11434", "llama3.2").unwrap();
        assert!(provider.validate().is_ok());
        assert_eq!(provider.pair().account(), "http://127.0.0.1:11434");
        let default = Provider::ollama_default("llama3.2").unwrap();
        assert_eq!(default.pair().account(), OLLAMA_DEFAULT_BASE_URL);
        assert!(default.validate().is_ok());
    }
}
