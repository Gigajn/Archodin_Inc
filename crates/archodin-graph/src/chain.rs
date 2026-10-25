//! Model chains (ADR 0005): the ordered Provider account and model pairs an Agent
//! uses. The Agent moves to the next pair only when a call fails with an error, a
//! rate limit or a timeout; a poor answer never triggers a fallback.

use std::sync::Arc;
use std::time::Duration;

use langchain_rust::language_models::llm::LLM;
use langchain_rust::language_models::GenerateResult;
use langchain_rust::schemas::Message;

use crate::agent::{AgentError, AgentTool, Provider};

/// One Provider account and model, as named in [`ChainEvent`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPair {
    account: String,
    model: String,
}

impl ModelPair {
    pub(crate) fn new(account: String, model: String) -> Self {
        Self { account, model }
    }

    /// The Provider account name (set with [`Provider::account`]).
    pub fn account(&self) -> &str {
        &self.account
    }

    /// The model name.
    pub fn model(&self) -> &str {
        &self.model
    }
}

/// Why an Agent left a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    /// The call failed; carries the error text.
    Error(String),
    /// The Provider answered 429 (rate limit).
    RateLimited,
    /// The call took longer than the chain's timeout.
    Timeout,
}

/// Something that happened while an Agent walked its Model chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEvent {
    /// `failed` could not answer, so the Agent moved on to `chosen`.
    Fallback {
        failed: ModelPair,
        chosen: ModelPair,
        reason: FallbackReason,
    },
}

/// The ordered list of Provider account and model pairs an Agent uses. Two
/// accounts of the same Provider can sit in one chain.
#[derive(Debug, Clone)]
pub struct ModelChain {
    pairs: Vec<Provider>,
    timeout: Option<Duration>,
}

impl ModelChain {
    /// A chain that starts with `first`.
    pub fn new(first: Provider) -> Self {
        Self {
            pairs: vec![first],
            timeout: None,
        }
    }

    /// A chain of `pairs` in order, or `None` when there are none.
    pub fn from_pairs(pairs: Vec<Provider>) -> Option<Self> {
        if pairs.is_empty() {
            return None;
        }
        Some(Self {
            pairs,
            timeout: None,
        })
    }

    /// Appends the next pair to try.
    pub fn then(mut self, next: Provider) -> Self {
        self.pairs.push(next);
        self
    }

    /// Treats a call that takes longer than `limit` as a failure of its pair.
    pub fn timeout(mut self, limit: Duration) -> Self {
        self.timeout = Some(limit);
        self
    }

    pub(crate) fn provider(&self, index: usize) -> &Provider {
        &self.pairs[index]
    }

    /// Calls the model, starting at pair `*current` and advancing it on failures.
    pub(crate) async fn generate(
        &self,
        current: &mut usize,
        tools: &[AgentTool],
        messages: &[Message],
        report: Option<&Arc<dyn Fn(ChainEvent) + Send + Sync>>,
    ) -> Result<GenerateResult, AgentError> {
        loop {
            let provider = &self.pairs[*current];
            let llm = provider.llm(tools);
            let call = llm.generate(messages);
            let outcome = match self.timeout {
                Some(limit) => match tokio::time::timeout(limit, call).await {
                    Ok(result) => result.map_err(|e| classify(e.to_string())),
                    Err(_) => Err(FallbackReason::Timeout),
                },
                None => call.await.map_err(|e| classify(e.to_string())),
            };
            let reason = match outcome {
                Ok(result) => return Ok(result),
                Err(reason) => reason,
            };
            let Some(next) = self.pairs.get(*current + 1) else {
                return Err(AgentError::Provider(match reason {
                    FallbackReason::Error(text) => text,
                    FallbackReason::RateLimited => "rate limited".into(),
                    FallbackReason::Timeout => "timed out".into(),
                }));
            };
            if let Some(report) = report {
                report(ChainEvent::Fallback {
                    failed: provider.pair(),
                    chosen: next.pair(),
                    reason,
                });
            }
            *current += 1;
        }
    }
}

fn classify(error: String) -> FallbackReason {
    let lower = error.to_lowercase();
    if ["429", "rate limit", "rate_limit", "too many requests"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        FallbackReason::RateLimited
    } else {
        FallbackReason::Error(error)
    }
}
