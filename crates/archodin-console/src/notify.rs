//! Notifications: when something waits for approval, a message goes out by
//! Telegram or email with a link straight to it.
//!
//! A [`Notifier`] sends one [`Notification`]. [`TelegramNotifier`] calls the Bot
//! API and reads the bot token from the [`Vault`] at send time. [`EmailNotifier`]
//! hands the message to a [`Mailer`]; an SMTP client implements that one-method
//! trait (credentials from the vault), and tests use a fake. The Agora groups a
//! burst of approvals into one notification (see `AgoraBuilder::notify_window`).

use std::fmt;
use std::time::Duration;

use archodin_graph::Vault;
use futures::future::BoxFuture;
use serde_json::json;

/// A message for the person: a subject line and a body that holds the deep links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// One line saying what is waiting.
    pub subject: String,
    /// The details, one approval per entry, each with its link.
    pub body: String,
}

/// Why a notification could not be sent. It never contains a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyError(pub String);

impl fmt::Display for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotifyError {}

/// Sends a [`Notification`] to the person.
pub trait Notifier: Send + Sync {
    /// Sends the notification.
    fn send<'a>(&'a self, notification: &'a Notification)
        -> BoxFuture<'a, Result<(), NotifyError>>;
}

/// Sends email; implement it over SMTP, or with a fake in tests.
pub trait Mailer: Send + Sync {
    /// Sends one plain-text email.
    fn send(&self, to: &str, subject: &str, body: &str) -> BoxFuture<'_, Result<(), NotifyError>>;
}

/// Notifies by email through a [`Mailer`].
pub struct EmailNotifier<M> {
    mailer: M,
    to: String,
}

impl<M: Mailer> EmailNotifier<M> {
    /// Sends to the address `to`.
    pub fn new(mailer: M, to: impl Into<String>) -> Self {
        Self {
            mailer,
            to: to.into(),
        }
    }
}

impl<M: Mailer> Notifier for EmailNotifier<M> {
    fn send<'a>(&'a self, n: &'a Notification) -> BoxFuture<'a, Result<(), NotifyError>> {
        self.mailer.send(&self.to, &n.subject, &n.body)
    }
}

/// Notifies through the Telegram Bot API.
pub struct TelegramNotifier {
    vault: Vault,
    token_secret: String,
    chat_id: String,
    api_base: String,
    client: reqwest::Client,
}

/// Telegram refuses messages longer than this many characters.
const TELEGRAM_LIMIT: usize = 4096;

impl TelegramNotifier {
    /// Sends to `chat_id` with the bot whose token is the vault secret
    /// `token_secret`.
    pub fn new(vault: Vault, token_secret: impl Into<String>, chat_id: impl Into<String>) -> Self {
        Self {
            vault,
            token_secret: token_secret.into(),
            chat_id: chat_id.into(),
            api_base: "https://api.telegram.org".into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("a plain client always builds"),
        }
    }

    /// Points at another Bot API server (a local one, or a mock in tests).
    pub fn api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into().trim_end_matches('/').to_string();
        self
    }
}

impl Notifier for TelegramNotifier {
    fn send<'a>(&'a self, n: &'a Notification) -> BoxFuture<'a, Result<(), NotifyError>> {
        Box::pin(async move {
            let token = self
                .vault
                .resolve(&self.token_secret)
                .map_err(|e| NotifyError(format!("Telegram: {e}")))?;
            let text: String = format!("{}\n\n{}", n.subject, n.body)
                .chars()
                .take(TELEGRAM_LIMIT)
                .collect();
            let url = format!("{}/bot{}/sendMessage", self.api_base, token.expose());
            let response = self
                .client
                .post(url)
                .json(&json!({
                    "chat_id": self.chat_id,
                    "text": text,
                    "disable_web_page_preview": true,
                }))
                .send()
                .await
                // The error would otherwise carry the URL, and the URL the token.
                .map_err(|e| NotifyError(format!("Telegram: {}", e.without_url())))?;
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(NotifyError(format!("Telegram answered {status}")))
            }
        })
    }
}
