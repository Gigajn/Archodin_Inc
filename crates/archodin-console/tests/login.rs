//! Login: every Agora page and API call needs a session.

use std::net::SocketAddr;
use std::time::Duration;

use archodin_console::{hash_password, Agora, Auth};
use archodin_graph::Vault;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple";

fn auth() -> Auth {
    let vault = Vault::new([9; 32]);
    vault.put("agora/username", "kudo");
    vault.put("agora/password_hash", &hash_password(PASSWORD));
    Auth::from_vault(&vault, "agora/username", "agora/password_hash").unwrap()
}

/// Same login, but with a short lockout so the lockout tests run in seconds.
fn auth_with_lockout() -> Auth {
    auth().failed_login_limit(5).lockout_base(Duration::from_secs(2))
}

fn app_with(auth: Auth) -> Router {
    Agora::builder()
        .runner(|_| Box::pin(async {}))
        .auth(auth)
        .build()
        .router()
}

fn app() -> Router {
    app_with(auth())
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Reply {
    send_with_ip(app, method, uri, headers, body, None).await
}

async fn send_with_ip(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
    ip: Option<SocketAddr>,
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(addr) = ip {
        req = req.extension(ConnectInfo(addr));
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let (parts, body) = res.into_parts();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap(),
    }
}

async fn login(app: &Router, user: &str, password: &str) -> Reply {
    login_ip(app, user, password, None).await
}

async fn login_ip(
    app: &Router,
    user: &str,
    password: &str,
    ip: Option<SocketAddr>,
) -> Reply {
    send_with_ip(
        app,
        "POST",
        "/login",
        &[],
        Some(json!({"username": user, "password": password})),
        ip,
    )
    .await
}

fn cookie_of(reply: &Reply) -> String {
    let set = reply.headers[header::SET_COOKIE].to_str().unwrap();
    set.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn every_page_and_api_call_needs_a_session() {
    let app = app();
    let api = [
        ("GET", "/api/jobs"),
        ("POST", "/api/jobs"),
        ("GET", "/api/jobs/1"),
        ("GET", "/api/jobs/1/events"),
        ("POST", "/api/jobs/1/cancel"),
        ("GET", "/api/approvals"),
        ("POST", "/api/approvals/1/approve"),
        ("POST", "/api/approvals/1/reject"),
        ("POST", "/api/roles/scout/approve-all"),
        ("POST", "/api/roles/scout/reject-all"),
        ("GET", "/api/usage"),
        ("GET", "/api/anything-else"),
    ];
    for (method, uri) in api {
        let r = send(&app, method, uri, &[], None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert!(!r.body.contains("Agora"), "{uri} leaked the page");
    }
    for uri in ["/", "/approvals/3", "/nope"] {
        let r = send(&app, "GET", uri, &[], None).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{uri}");
        let location = r.headers[header::LOCATION].to_str().unwrap();
        assert!(location.starts_with("/login?next="), "{location}");
    }
    let r = send(&app, "GET", "/approvals/3", &[], None).await;
    assert_eq!(r.headers[header::LOCATION], "/login?next=%2Fapprovals%2F3");
}

#[tokio::test]
async fn the_login_page_is_open_and_wrong_credentials_get_nothing() {
    let app = app();
    let page = send(&app, "GET", "/login", &[], None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("<form"));

    for (user, password) in [("kudo", "wrong"), ("someone", PASSWORD), ("", "")] {
        let r = login(&app, user, password).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{user}");
        assert!(r.headers.get(header::SET_COOKIE).is_none());
        assert!(!r.body.contains("token"));
    }
}

#[tokio::test]
async fn a_session_cookie_opens_pages_and_the_api_until_logout() {
    let app = app();
    let r = login(&app, "kudo", PASSWORD).await;
    assert_eq!(r.status, StatusCode::OK);
    let set = r.headers[header::SET_COOKIE].to_str().unwrap().to_string();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Secure"));
    let cookie = cookie_of(&r);

    let jobs = send(&app, "GET", "/api/jobs", &[("cookie", &cookie)], None).await;
    assert_eq!(jobs.status, StatusCode::OK);
    let page = send(&app, "GET", "/", &[("cookie", &cookie)], None).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("Agora"));
    let deep = send(&app, "GET", "/approvals/7", &[("cookie", &cookie)], None).await;
    assert_eq!(deep.status, StatusCode::OK);

    let out = send(&app, "POST", "/logout", &[("cookie", &cookie)], None).await;
    assert_eq!(out.status, StatusCode::OK);
    let after = send(&app, "GET", "/api/jobs", &[("cookie", &cookie)], None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_bearer_token_from_login_works_and_guesses_do_not() {
    let app = app();
    let r = login(&app, "kudo", PASSWORD).await;
    let token = serde_json::from_str::<Value>(&r.body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(token.len() >= 32);

    let good = format!("Bearer {token}");
    let ok = send(&app, "GET", "/api/usage", &[("authorization", &good)], None).await;
    assert_eq!(ok.status, StatusCode::OK);

    let longer = format!("Bearer {token}x");
    for bad in ["Bearer nope", "Bearer ", "Basic abc", longer.as_str()] {
        let r = send(&app, "GET", "/api/usage", &[("authorization", bad)], None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{bad}");
    }
    let r = send(
        &app,
        "GET",
        "/api/usage",
        &[("cookie", "agora_session=nope")],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sessions_expire() {
    let app = app_with(auth().session_lifetime(Duration::from_millis(50)));
    let cookie = cookie_of(&login(&app, "kudo", PASSWORD).await);
    let status = |app: &Router, cookie: &str| {
        let (app, cookie) = (app.clone(), cookie.to_string());
        async move {
            send(&app, "GET", "/api/usage", &[("cookie", &cookie)], None)
                .await
                .status
        }
    };
    assert_eq!(status(&app, &cookie).await, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(status(&app, &cookie).await, StatusCode::UNAUTHORIZED);
}

#[test]
fn the_vault_must_hold_the_login() {
    let vault = Vault::new([1; 32]);
    assert!(Auth::from_vault(&vault, "u", "h").is_err());
    vault.put("u", "kudo");
    vault.put("h", "not a password hash");
    assert!(Auth::from_vault(&vault, "u", "h").is_err());
}

#[tokio::test]
async fn five_failed_logins_lock_out_the_sixth_even_with_the_right_password() {
    let app = app_with(auth_with_lockout());
    for n in 1..=4 {
        let r = login(&app, "kudo", "wrong").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "attempt {n}");
    }
    // The fifth failure trips the lockout.
    let fifth = login(&app, "kudo", "wrong").await;
    assert_eq!(fifth.status, StatusCode::TOO_MANY_REQUESTS);
    // The sixth attempt is refused even with the right password.
    let sixth = login(&app, "kudo", PASSWORD).await;
    assert_eq!(sixth.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(sixth.headers.get(header::SET_COOKIE).is_none());
    assert!(!sixth.body.contains("token"));
    // Once the lockout lapses, the right password works again.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(login(&app, "kudo", PASSWORD).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_lockout_is_per_ip_and_username() {
    let app = app_with(auth_with_lockout());
    let a: SocketAddr = "10.0.0.1:1234".parse().unwrap();
    let b: SocketAddr = "10.0.0.2:1234".parse().unwrap();
    for _ in 0..5 {
        login_ip(&app, "kudo", "wrong", Some(a)).await;
    }
    // The same username from another IP is not locked out.
    assert_eq!(
        login_ip(&app, "kudo", PASSWORD, Some(b)).await.status,
        StatusCode::OK
    );
    // ...but from the locked IP the right password is still refused.
    assert_eq!(
        login_ip(&app, "kudo", PASSWORD, Some(a)).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another username from the locked IP is a different key: refused as
    // wrong credentials, not as locked out.
    assert_eq!(
        login_ip(&app, "someone", PASSWORD, Some(a)).await.status,
        StatusCode::UNAUTHORIZED
    );
}
