//! The Model Registry (ADR 0023): the one place model choice lives.
//!
//! A [`Registry`] holds NON-secret data only: model accounts (kind, base URL,
//! default model, Vault secret *name*) and the ordered model chains per Role,
//! with per-Agent overrides and one explicit global default. Keys NEVER enter
//! this module — only the Vault holds values, referenced by name
//! (`model/<kind>/<account>`).
//!
//! Resolution order, with no silent fallback: Agent override → Role chain →
//! global default. An empty result is [`ModelError::ModelNotConfigured`],
//! never a guessed model. The Registry stores *references* (account name +
//! model name), never a [`Provider`](crate::Provider): a Provider is built
//! from Registry + Vault each time one is needed, so rotation and revoke
//! apply without a restart (ADR 0023 §7).
//!
//! Validation here is shape only, never network (ADR 0023 §5): an account
//! without a kind, a base URL that fails
//! [`validate_endpoint`](crate::http_guard::validate_endpoint), an empty
//! model name, or a chain naming an unknown account is refused when it is
//! registered.

use std::collections::HashMap;

use crate::http_guard::validate_endpoint;

/// The kind of a model account: which provider API it speaks (ADR 0022).
/// Only these four kinds exist in v1; Bedrock and Vertex AI are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    /// Any vendor reached by base URL, API key and model name.
    OpenAiCompatible,
    /// Gemini through the native client (`gemini.rs`).
    Gemini,
    /// Claude through the native client (`native.rs`).
    Claude,
    /// A local Ollama server, no key.
    Ollama,
}

impl ModelKind {
    /// The kind as it is written in configuration: `openai_compatible`,
    /// `gemini`, `claude`, `ollama`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::Gemini => "gemini",
            Self::Claude => "claude",
            Self::Ollama => "ollama",
        }
    }

    /// Parses the configuration spelling; `None` is "an account without a
    /// kind" (ADR 0023 §5) — the caller must refuse it, not guess a kind.
    pub fn from_str(text: &str) -> Option<Self> {
        match text {
            "openai_compatible" => Some(Self::OpenAiCompatible),
            "gemini" => Some(Self::Gemini),
            "claude" => Some(Self::Claude),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

impl std::fmt::Display for ModelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a model could not be resolved or used (ADR 0023 §4).
///
/// Every variant carries the provider kind, the account name and the model
/// name — and NEVER a key value, and NEVER a raw upstream error body
/// (redact first, ADR 0025 §1). The `Display` text is built only from
/// those fields, so it is safe to carry into logs, events and API errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// No chain resolves for this Agent/Role, or a registered account or
    /// chain is not shaped right. `kind`/`account`/`model` name the account
    /// when the failure is about one (for example a bad base URL at
    /// registration); they are empty when no chain exists at all.
    #[error("model not configured: {context}")]
    ModelNotConfigured {
        kind: Option<ModelKind>,
        account: String,
        model: String,
        context: String,
    },
    /// The account exists but its key is missing — revoked, or the Vault is
    /// locked / the secret was never provisioned.
    #[error("the key for {kind} account `{account}` (model `{model}`) is missing")]
    ModelKeyMissing {
        kind: ModelKind,
        account: String,
        model: String,
    },
    /// The provider answered 401/403: the key is wrong or was revoked at the
    /// vendor. Rotate it (ADR 0025 §5), do not retry the same key.
    #[error("the {kind} provider refused the key for account `{account}` (model `{model}`)")]
    ProviderAuthFailed {
        kind: ModelKind,
        account: String,
        model: String,
    },
    /// The provider answered 404: the model name is wrong.
    #[error("model `{model}` was not found on {kind} account `{account}`")]
    ModelNotFound {
        kind: ModelKind,
        account: String,
        model: String,
    },
    /// The network failed, timed out, or the provider answered a transient
    /// status (429, 5xx): nothing about the configuration is known to be
    /// wrong, so the chain may try its next pair.
    #[error("{kind} account `{account}` (model `{model}`) could not be reached")]
    ProviderUnavailable {
        kind: ModelKind,
        account: String,
        model: String,
    },
}

impl ModelError {
    /// No chain resolves for `agent` with `role`.
    pub fn not_configured(agent: &str, role: &str) -> Self {
        Self::ModelNotConfigured {
            kind: None,
            account: String::new(),
            model: String::new(),
            context: format!("no model chain for agent `{agent}` with role `{role}`"),
        }
    }

    /// A registered account or chain is not shaped right (`context` says
    /// how); `kind`/`account`/`model` name the account it is about.
    pub fn invalid(
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self::ModelNotConfigured {
            kind: Some(kind),
            account: account.into(),
            model: model.into(),
            context: context.into(),
        }
    }

    /// The key for this account/model is missing.
    pub fn key_missing(
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::ModelKeyMissing {
            kind,
            account: account.into(),
            model: model.into(),
        }
    }

    /// The provider refused the key (HTTP 401/403).
    pub fn auth_failed(
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::ProviderAuthFailed {
            kind,
            account: account.into(),
            model: model.into(),
        }
    }

    /// The model name does not exist at the provider (HTTP 404).
    pub fn not_found(
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::ModelNotFound {
            kind,
            account: account.into(),
            model: model.into(),
        }
    }

    /// The provider could not be reached, or answered transiently.
    pub fn unavailable(
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::ProviderUnavailable {
            kind,
            account: account.into(),
            model: model.into(),
        }
    }

    /// Maps an HTTP status to the typed error: 401/403 → `ProviderAuthFailed`,
    /// 404 → `ModelNotFound`, anything else (429, 5xx, …) → `ProviderUnavailable`.
    /// The upstream body is deliberately NOT carried: it may contain a key.
    pub fn from_status(
        status: u16,
        kind: ModelKind,
        account: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        match status {
            401 | 403 => Self::auth_failed(kind, account, model),
            404 => Self::not_found(kind, account, model),
            _ => Self::unavailable(kind, account, model),
        }
    }

    /// The provider kind, when the failure names one.
    pub fn kind(&self) -> Option<ModelKind> {
        match self {
            Self::ModelNotConfigured { kind, .. } => *kind,
            Self::ModelKeyMissing { kind, .. }
            | Self::ProviderAuthFailed { kind, .. }
            | Self::ModelNotFound { kind, .. }
            | Self::ProviderUnavailable { kind, .. } => Some(*kind),
        }
    }

    /// The account name, when the failure names one.
    pub fn account(&self) -> &str {
        match self {
            Self::ModelNotConfigured { account, .. }
            | Self::ModelKeyMissing { account, .. }
            | Self::ProviderAuthFailed { account, .. }
            | Self::ModelNotFound { account, .. }
            | Self::ProviderUnavailable { account, .. } => account,
        }
    }

    /// The model name, when the failure names one.
    pub fn model(&self) -> &str {
        match self {
            Self::ModelNotConfigured { model, .. }
            | Self::ModelKeyMissing { model, .. }
            | Self::ProviderAuthFailed { model, .. }
            | Self::ModelNotFound { model, .. }
            | Self::ProviderUnavailable { model, .. } => model,
        }
    }
}

/// One model account: a single credential for one provider.
///
/// NON-secret data only. The key itself lives in the Vault under
/// [`ModelAccount::vault_secret`] (`model/<kind>/<account>`); this struct
/// holds the name, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAccount {
    /// The account name, for example `work` or `groq-main`.
    pub name: String,
    /// Which provider API this account speaks.
    pub kind: ModelKind,
    /// The API root, for example `https://api.openai.com/v1`.
    pub base_url: String,
    /// The model used when a chain names this account without a model of
    /// its own — never empty.
    pub default_model: String,
    /// The Vault secret name holding the key, `model/<kind>/<account>`.
    /// Empty only for `ollama`, which needs no key.
    pub vault_secret: String,
}

impl ModelAccount {
    /// Builds an account, deriving the Vault secret name from kind + name.
    /// Use [`ModelAccount::with_vault_secret`] only when the secret was
    /// provisioned under a different name.
    pub fn new(
        name: impl Into<String>,
        kind: ModelKind,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let vault_secret = Self::vault_secret_name(kind, &name);
        Self {
            name,
            kind,
            base_url: base_url.into(),
            default_model: default_model.into(),
            vault_secret,
        }
    }

    /// Builds an account with an explicit Vault secret name.
    pub fn with_vault_secret(
        name: impl Into<String>,
        kind: ModelKind,
        base_url: impl Into<String>,
        default_model: impl Into<String>,
        vault_secret: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            base_url: base_url.into(),
            default_model: default_model.into(),
            vault_secret: vault_secret.into(),
        }
    }

    /// The Vault secret name for an account: `model/<kind>/<account>`.
    pub fn vault_secret_name(kind: ModelKind, account: &str) -> String {
        format!("model/{}/{}", kind.as_str(), account)
    }

    /// Shape validation, no network (ADR 0023 §5): a name, a base URL that
    /// passes [`validate_endpoint`], and — for every kind except `ollama` —
    /// a Vault secret name. The default model MAY be empty (ADR 0020: the
    /// model may be set per Role in the chain); [`Registry::resolve`] uses
    /// the chain's explicit model and never this default, so an empty
    /// default does not block registration. A Provider built from an
    /// account with an empty default and no chain model must fail clearly
    /// at that construction site — it is not this function's job.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.name.is_empty() {
            return Err(ModelError::invalid(
                self.kind,
                &self.name,
                &self.default_model,
                "an account name must not be empty",
            ));
        }
        if let Err(e) = validate_endpoint(&self.base_url) {
            return Err(ModelError::invalid(
                self.kind,
                &self.name,
                &self.default_model,
                format!("base URL refused: {e}"),
            ));
        }
        if self.kind != ModelKind::Ollama && self.vault_secret.is_empty() {
            return Err(ModelError::invalid(
                self.kind,
                &self.name,
                &self.default_model,
                "a Vault secret name is required (ollama is the only kind without a key)",
            ));
        }
        Ok(())
    }
}

/// One entry of a model chain: `(account name, model name)`. The model is
/// always explicit — an empty model is refused at registration, it never
/// silently means "the account default".
pub type ChainEntry = (String, String);

/// The Model Registry: accounts, chains per Role, overrides per Agent, and
/// one explicit global default (which may be empty on a fresh install).
#[derive(Debug, Clone, Default)]
pub struct Registry {
    accounts: HashMap<String, ModelAccount>,
    role_chains: HashMap<String, Vec<ChainEntry>>,
    agent_overrides: HashMap<String, Vec<ChainEntry>>,
    global_default: Vec<ChainEntry>,
}

impl Registry {
    /// An empty Registry: no accounts, no chains, no global default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an account, replacing any account with the same name.
    /// Refuses a mis-shaped account (see [`ModelAccount::validate`]).
    pub fn add_account(&mut self, account: ModelAccount) -> Result<(), ModelError> {
        account.validate()?;
        self.accounts.insert(account.name.clone(), account);
        Ok(())
    }

    /// The account called `name`, if one is registered.
    pub fn account(&self, name: &str) -> Option<&ModelAccount> {
        self.accounts.get(name)
    }

    /// Every registered account, ordered by name (for the Agora list).
    pub fn accounts(&self) -> Vec<ModelAccount> {
        let mut accounts: Vec<ModelAccount> = self.accounts.values().cloned().collect();
        accounts.sort_by(|a, b| a.name.cmp(&b.name));
        accounts
    }

    /// Removes the account called `name` — the Registry half of a revoke
    /// (ADR 0024): the caller must also remove the key from the Vault,
    /// or the key survives as an orphan. Returns the removed account, or
    /// `None` when there was none. Chains naming the account are NOT
    /// rewritten: resolving them then fails closed with
    /// `ModelNotConfigured` (no silent fallback to another provider),
    /// and the operator re-points the chain explicitly.
    pub fn remove_account(&mut self, name: &str) -> Option<ModelAccount> {
        self.accounts.remove(name)
    }

    /// Sets the model chain for a Role. Every entry must name a registered
    /// account with a non-empty model; an empty chain clears the Role's
    /// chain (its Agents then fall through to the global default).
    pub fn set_role_chain(&mut self, role: &str, chain: Vec<ChainEntry>) -> Result<(), ModelError> {
        self.check_chain(role, &chain)?;
        self.role_chains.insert(role.to_string(), chain);
        Ok(())
    }

    /// Sets the model chain override for one Agent. Same rules as
    /// [`Registry::set_role_chain`]; an empty chain clears the override.
    pub fn set_agent_override(
        &mut self,
        agent: &str,
        chain: Vec<ChainEntry>,
    ) -> Result<(), ModelError> {
        self.check_chain(agent, &chain)?;
        self.agent_overrides.insert(agent.to_string(), chain);
        Ok(())
    }

    /// Sets the global default chain — the chain used when neither an Agent
    /// override nor a Role chain exists. May be empty on a fresh install;
    /// resolving then fails with [`ModelError::ModelNotConfigured`].
    pub fn set_global_default(&mut self, chain: Vec<ChainEntry>) -> Result<(), ModelError> {
        self.check_chain("global default", &chain)?;
        self.global_default = chain;
        Ok(())
    }

    /// Resolves the model chain for `agent` with `role`, in order: the
    /// Agent's override, then the Role's chain, then the global default.
    /// An empty chain at one level falls through to the next; when all
    /// three are empty the result is `ModelNotConfigured` — never a guess.
    /// The returned pairs are `(account, model name)` in chain order.
    pub fn resolve(
        &self,
        agent: &str,
        role: &str,
    ) -> Result<Vec<(ModelAccount, String)>, ModelError> {
        let chain = self
            .agent_overrides
            .get(agent)
            .filter(|c| !c.is_empty())
            .or_else(|| self.role_chains.get(role).filter(|c| !c.is_empty()))
            .or_else(|| {
                if self.global_default.is_empty() {
                    None
                } else {
                    Some(&self.global_default)
                }
            })
            .ok_or_else(|| ModelError::not_configured(agent, role))?;
        chain
            .iter()
            .map(|(account_name, model)| {
                let account = self.accounts.get(account_name).ok_or_else(|| {
                    ModelError::invalid(
                        ModelKind::OpenAiCompatible,
                        account_name,
                        model,
                        format!("chain for agent `{agent}` names unknown account `{account_name}`"),
                    )
                })?;
                Ok((account.clone(), model.clone()))
            })
            .collect()
    }

    /// Checks a chain before it is stored: every entry names a registered
    /// account and a non-empty model.
    fn check_chain(&self, owner: &str, chain: &[ChainEntry]) -> Result<(), ModelError> {
        for (account_name, model) in chain {
            let Some(account) = self.accounts.get(account_name) else {
                return Err(ModelError::invalid(
                    ModelKind::OpenAiCompatible,
                    account_name,
                    model,
                    format!("chain for `{owner}` names unknown account `{account_name}`"),
                ));
            };
            if model.is_empty() {
                return Err(ModelError::invalid(
                    account.kind,
                    account_name,
                    model,
                    format!(
                        "chain for `{owner}` names account `{account_name}` with an empty model"
                    ),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str) -> ModelAccount {
        ModelAccount::new(
            name,
            ModelKind::OpenAiCompatible,
            "https://api.example.com/v1",
            "m1",
        )
    }

    fn registry_with_accounts() -> Registry {
        let mut registry = Registry::new();
        registry.add_account(account("work")).unwrap();
        registry.add_account(account("spare")).unwrap();
        registry
    }

    #[test]
    fn an_empty_registry_resolves_to_model_not_configured() {
        let registry = Registry::new();
        let err = registry.resolve("nova", "writer").unwrap_err();
        assert_eq!(err, ModelError::not_configured("nova", "writer"));
        assert_eq!(err.kind(), None);
        assert_eq!(err.account(), "");
        assert_eq!(err.model(), "");
    }

    #[test]
    fn the_role_chain_resolves_in_order() {
        let mut registry = registry_with_accounts();
        registry
            .set_role_chain(
                "writer",
                vec![("work".into(), "m1".into()), ("spare".into(), "m2".into())],
            )
            .unwrap();
        let resolved = registry.resolve("nova", "writer").unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0.name, "work");
        assert_eq!(resolved[0].1, "m1");
        assert_eq!(resolved[1].0.name, "spare");
        assert_eq!(resolved[1].1, "m2");
    }

    #[test]
    fn the_agent_override_wins_over_the_role_chain() {
        let mut registry = registry_with_accounts();
        registry
            .set_role_chain("writer", vec![("work".into(), "m1".into())])
            .unwrap();
        registry
            .set_agent_override("nova", vec![("spare".into(), "m2".into())])
            .unwrap();
        let resolved = registry.resolve("nova", "writer").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.name, "spare");
        // Another Agent of the same Role still gets the Role chain.
        let other = registry.resolve("other", "writer").unwrap();
        assert_eq!(other[0].0.name, "work");
    }

    #[test]
    fn the_global_default_is_used_when_nothing_else_is_set() {
        let mut registry = registry_with_accounts();
        registry
            .set_global_default(vec![("spare".into(), "m2".into())])
            .unwrap();
        let resolved = registry.resolve("nova", "writer").unwrap();
        assert_eq!(resolved[0].0.name, "spare");
        assert_eq!(resolved[0].1, "m2");
    }

    #[test]
    fn an_empty_chain_at_one_level_falls_through_to_the_next() {
        let mut registry = registry_with_accounts();
        registry.set_role_chain("writer", vec![]).unwrap();
        registry
            .set_global_default(vec![("work".into(), "m1".into())])
            .unwrap();
        let resolved = registry.resolve("nova", "writer").unwrap();
        assert_eq!(resolved[0].0.name, "work");
    }

    #[test]
    fn an_account_with_a_bad_base_url_is_refused() {
        let mut registry = Registry::new();
        let bad = ModelAccount::new("bad", ModelKind::OpenAiCompatible, "not a url", "m1");
        let err = registry.add_account(bad).unwrap_err();
        assert!(matches!(err, ModelError::ModelNotConfigured { .. }));
        assert!(err.to_string().contains("refused"), "{err}");
        assert!(registry.account("bad").is_none());
    }

    #[test]
    fn an_account_with_an_empty_model_is_accepted_but_a_chain_entry_without_one_is_refused() {
        // ADR 0020: the model may be set per Role in the chain, so an empty
        // default must not block registration — but a chain entry with an
        // empty model still is, because resolve() has nothing to return.
        let mut registry = Registry::new();
        let account = ModelAccount::new(
            "work",
            ModelKind::OpenAiCompatible,
            "https://api.example.com/v1",
            "",
        );
        registry.add_account(account).unwrap();
        assert!(registry.account("work").is_some());
        assert!(matches!(
            registry
                .set_role_chain("writer", vec![("work".to_string(), "".to_string())])
                .unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
    }

    #[test]
    fn a_non_ollama_account_without_a_vault_secret_is_refused() {
        let mut registry = Registry::new();
        let bad = ModelAccount::with_vault_secret(
            "bad",
            ModelKind::Claude,
            "https://api.anthropic.com",
            "m1",
            "",
        );
        assert!(matches!(
            registry.add_account(bad).unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
    }

    #[test]
    fn an_ollama_account_needs_no_vault_secret() {
        let mut registry = Registry::new();
        let ollama = ModelAccount::with_vault_secret(
            "local",
            ModelKind::Ollama,
            "http://127.0.0.1:11434",
            "llama3.2",
            "",
        );
        registry.add_account(ollama).unwrap();
        assert!(registry.account("local").is_some());
    }

    #[test]
    fn a_chain_naming_an_unknown_account_is_refused() {
        let mut registry = Registry::new();
        let err = registry
            .set_role_chain("writer", vec![("ghost".into(), "m1".into())])
            .unwrap_err();
        assert!(matches!(err, ModelError::ModelNotConfigured { .. }));
        assert!(err.to_string().contains("ghost"), "{err}");
    }

    #[test]
    fn a_chain_entry_with_an_empty_model_is_refused() {
        let mut registry = registry_with_accounts();
        assert!(matches!(
            registry
                .set_role_chain("writer", vec![("work".into(), "".into())])
                .unwrap_err(),
            ModelError::ModelNotConfigured { .. }
        ));
    }

    #[test]
    fn kind_spellings_round_trip_and_unknown_is_none() {
        for kind in [
            ModelKind::OpenAiCompatible,
            ModelKind::Gemini,
            ModelKind::Claude,
            ModelKind::Ollama,
        ] {
            assert_eq!(ModelKind::from_str(kind.as_str()), Some(kind));
        }
        assert_eq!(ModelKind::from_str("bedrock"), None);
        assert_eq!(ModelKind::from_str(""), None);
        assert_eq!(
            ModelAccount::vault_secret_name(ModelKind::Claude, "work"),
            "model/claude/work"
        );
    }

    #[test]
    fn http_statuses_map_to_the_typed_errors() {
        let auth = ModelError::from_status(401, ModelKind::Claude, "work", "m1");
        assert_eq!(
            auth,
            ModelError::auth_failed(ModelKind::Claude, "work", "m1")
        );
        assert_eq!(
            ModelError::from_status(403, ModelKind::Claude, "work", "m1"),
            auth
        );
        assert_eq!(
            ModelError::from_status(404, ModelKind::Gemini, "work", "m1"),
            ModelError::not_found(ModelKind::Gemini, "work", "m1")
        );
        assert_eq!(
            ModelError::from_status(429, ModelKind::Ollama, "local", "m1"),
            ModelError::unavailable(ModelKind::Ollama, "local", "m1")
        );
        assert_eq!(
            ModelError::from_status(500, ModelKind::Ollama, "local", "m1"),
            ModelError::unavailable(ModelKind::Ollama, "local", "m1")
        );
        assert_eq!(auth.kind(), Some(ModelKind::Claude));
        assert_eq!(auth.account(), "work");
        assert_eq!(auth.model(), "m1");
    }

    #[test]
    fn an_account_without_a_default_model_registers_and_resolves_through_a_chain() {
        // ADR 0020: the model may be set per Role in the chain, so an empty
        // default must not block registration; resolve() uses the chain's
        // explicit model, never the default.
        let mut registry = Registry::new();
        let account = ModelAccount::new(
            "work",
            ModelKind::OpenAiCompatible,
            "https://api.example.com/v1",
            "",
        );
        registry.add_account(account).unwrap();
        assert_eq!(
            registry.account("work").unwrap().default_model,
            "",
            "empty default is kept, not replaced"
        );
        registry
            .set_role_chain("writer", vec![("work".to_string(), "m-chain".to_string())])
            .unwrap();
        let resolved = registry.resolve("nova", "writer").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.name, "work");
        assert_eq!(resolved[0].1, "m-chain");
    }

    #[test]
    fn a_canary_key_never_appears_in_registry_debug_or_resolve_errors() {
        // The canary is a fake key value, stored in the Vault under the
        // account's secret name; the Registry only ever holds the NAME.
        let canary = format!(
            "sk-canary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock runs forward")
                .as_nanos()
        );
        let vault = crate::Vault::new([7; 32].into());
        vault.put("model/openai_compatible/work", &canary);
        let mut registry = Registry::new();
        registry.add_account(account("work")).unwrap();
        assert!(!format!("{registry:?}").contains(&canary));
        assert!(!format!("{:?}", registry.account("work")).contains(&canary));
        let err = registry.resolve("nova", "writer").unwrap_err();
        assert!(!err.to_string().contains(&canary));
        assert!(!format!("{err:?}").contains(&canary));
        // And the Vault itself redacts the value it knows.
        assert!(!vault.redact(&format!("boom {canary}")).contains(&canary));
    }
}
