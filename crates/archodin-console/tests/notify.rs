//! Approval notifications: Telegram through a mock Bot API, email through a fake
//! mailer, bursts grouped into one message with a deep link per approval.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use archodin_console::{
    hash_password, Agora, Auth, EmailNotifier, Mailer, Notifier, NotifyError, TelegramNotifier,
};
use archodin_graph::{ApprovalRequest, Approver, Vault};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use mockito::{Matcher, Server};
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "123:SECRET-BOT-TOKEN";

fn vault() -> Vault {
    let vault = Vault::new([5; 32]);
    vault.put("telegram/bot_token", TOKEN);
    vault
}

fn request(node: &str, prompt: &str) -> ApprovalRequest {
    ApprovalRequest {
        job: "nakama".into(),
        node: node.into(),
        prompt: prompt.into(),
    }
}

fn agora_with(notifier: impl Notifier + 'static, window_ms: u64) -> Agora {
    Agora::builder()
        .runner(|_| Box::pin(async {}))
        .public_url("https://agora.test/")
        .notifier(Arc::new(notifier))
        .notify_window(Duration::from_millis(window_ms))
        .insecure_no_auth()
        .build()
}

fn telegram(server: &Server) -> TelegramNotifier {
    TelegramNotifier::new(vault(), "telegram/bot_token", "4242").api_base(server.url())
}

#[tokio::test]
async fn a_new_approval_is_sent_to_telegram_with_a_link_to_it() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/bot123:SECRET-BOT-TOKEN/sendMessage")
        .match_body(Matcher::PartialJson(json!({"chat_id": "4242"})))
        .match_body(Matcher::Regex(r"https://agora\.test/approvals/1".into()))
        .match_body(Matcher::Regex("Hire a Scout".into()))
        .with_body(r#"{"ok":true}"#)
        .expect(1)
        .create_async()
        .await;
    let agora = agora_with(telegram(&server), 20);

    agora.decide(&request("hire", "Hire a Scout")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    mock.assert_async().await;

    // Asking again about the same pending approval is not news.
    agora.decide(&request("hire", "Hire a Scout")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    mock.assert_async().await;
}

#[tokio::test]
async fn a_burst_of_approvals_is_one_message_with_every_link() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/bot123:SECRET-BOT-TOKEN/sendMessage")
        .match_body(Matcher::Regex(r"3 approvals".into()))
        .match_body(Matcher::Regex(r"approvals/1".into()))
        .match_body(Matcher::Regex(r"approvals/2".into()))
        .match_body(Matcher::Regex(r"approvals/3".into()))
        .with_body(r#"{"ok":true}"#)
        .expect(1)
        .create_async()
        .await;
    let agora = agora_with(telegram(&server), 150);

    for n in 1..=3 {
        agora
            .decide(&request(&format!("hire-{n}"), &format!("Hire #{n}")))
            .await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    mock.assert_async().await;

    // After the window closed, the next approval starts a new message.
    let second = server
        .mock("POST", "/bot123:SECRET-BOT-TOKEN/sendMessage")
        .match_body(Matcher::Regex(r"approvals/4".into()))
        .with_body(r#"{"ok":true}"#)
        .expect(1)
        .create_async()
        .await;
    agora.decide(&request("hire-4", "Hire #4")).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    second.assert_async().await;
}

#[tokio::test]
async fn a_failing_bot_api_is_recorded_without_leaking_the_token() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", Matcher::Any)
        .with_status(500)
        .with_body("boom")
        .create_async()
        .await;
    let agora = agora_with(telegram(&server), 10);
    agora.decide(&request("hire", "Hire a Scout")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let error = agora
        .last_notification_error()
        .expect("the failure is kept");
    assert!(error.contains("500"), "{error}");
    assert!(!error.contains("SECRET-BOT-TOKEN"), "{error}");
}

#[tokio::test]
async fn a_missing_vault_secret_is_an_error_not_a_request() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let notifier = TelegramNotifier::new(Vault::new([5; 32]), "telegram/bot_token", "1")
        .api_base(server.url());
    let agora = agora_with(notifier, 10);
    agora.decide(&request("hire", "Hire a Scout")).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    mock.assert_async().await;
    assert!(agora
        .last_notification_error()
        .unwrap()
        .contains("telegram/bot_token"));
}

#[derive(Clone, Default)]
struct FakeMailer(Arc<Mutex<Vec<(String, String, String)>>>);

impl Mailer for FakeMailer {
    fn send(&self, to: &str, subject: &str, body: &str) -> BoxFuture<'_, Result<(), NotifyError>> {
        self.0
            .lock()
            .unwrap()
            .push((to.into(), subject.into(), body.into()));
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn email_gets_the_same_grouped_message() {
    let mailer = FakeMailer::default();
    let agora = agora_with(EmailNotifier::new(mailer.clone(), "kudo@example.com"), 100);
    agora.decide(&request("a", "Approve skill A")).await;
    agora.decide(&request("b", "Approve skill B")).await;
    tokio::time::sleep(Duration::from_millis(400)).await;

    let sent = mailer.0.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let (to, subject, body) = &sent[0];
    assert_eq!(to, "kudo@example.com");
    assert!(subject.contains("2 approvals"), "{subject}");
    assert!(body.contains("https://agora.test/approvals/1") && body.contains("approvals/2"));
    assert!(body.contains("Approve skill A") && body.contains("Approve skill B"));
}

#[tokio::test]
async fn every_configured_notifier_gets_it_and_one_failing_does_not_block_the_other() {
    let mailer = FakeMailer::default();
    let agora = Agora::builder()
        .runner(|_| Box::pin(async {}))
        .public_url("https://agora.test")
        .notifier(Arc::new(TelegramNotifier::new(
            Vault::new([0; 32]),
            "missing",
            "1",
        )))
        .notifier(Arc::new(EmailNotifier::new(
            mailer.clone(),
            "kudo@example.com",
        )))
        .notify_window(Duration::from_millis(10))
        .insecure_no_auth()
        .build();
    agora.decide(&request("a", "Approve skill A")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(mailer.0.lock().unwrap().len(), 1);
    assert!(agora.last_notification_error().is_some());
}

#[tokio::test]
async fn the_link_in_the_message_opens_behind_the_login() {
    let vault = Vault::new([5; 32]);
    vault.put("u", "kudo");
    vault.put("h", &hash_password("pw"));
    let mailer = FakeMailer::default();
    let agora = Agora::builder()
        .runner(|_| Box::pin(async {}))
        .auth(Auth::from_vault(&vault, "u", "h").unwrap())
        .public_url("https://agora.test")
        .notifier(Arc::new(EmailNotifier::new(
            mailer.clone(),
            "k@example.com",
        )))
        .notify_window(Duration::from_millis(10))
        .build();
    let app = agora.router();
    agora.decide(&request("a", "Approve skill A")).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let body = mailer.0.lock().unwrap()[0].2.clone();
    let path = body
        .split_whitespace()
        .find_map(|w| w.strip_prefix("https://agora.test"))
        .unwrap()
        .to_string();

    let anon = app
        .clone()
        .oneshot(Request::get(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::SEE_OTHER);

    let login = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": "kudo", "password": "pw"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let page = app
        .clone()
        .oneshot(
            Request::get(&path)
                .header("cookie", cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);

    // The approval itself is in the list behind the same session.
    let list = app
        .oneshot(
            Request::get("/api/approvals")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = list.into_body().collect().await.unwrap().to_bytes();
    let groups: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(groups[0]["items"][0]["id"], 1);
}
