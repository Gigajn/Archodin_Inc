//! Login for the Agora: only the person who holds the password can open it.
//!
//! The username and the argon2 password hash live in the [`Vault`], never in plain
//! configuration ([`Auth::from_vault`]). A correct login creates a session; the
//! session is carried by an `HttpOnly` cookie or, for scripts, by an
//! `Authorization: Bearer` token (both come from `POST /login`). A middleware layer
//! puts every route, including unknown paths, behind the session.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use archodin_graph::Vault;
use archodin_graph::locks::mutex;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

const COOKIE: &str = "agora_session";
const LOGIN_PAGE: &str = include_str!("login.html");

/// Hashes a password with argon2id, for storing in the [`Vault`].
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hashing with default parameters cannot fail")
        .to_string()
}

/// Why the login could not be set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthError(String);

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AuthError {}

struct Shared {
    username: String,
    hash: String,
    sessions: Mutex<HashMap<String, Instant>>,
}

/// The Agora's login. Clones share the same sessions.
#[derive(Clone)]
pub struct Auth {
    shared: Arc<Shared>,
    lifetime: Duration,
    secure_cookie: bool,
}

impl Auth {
    /// Reads the username and the argon2 hash (see [`hash_password`]) from the
    /// vault secrets called `username_secret` and `hash_secret`.
    pub fn from_vault(
        vault: &Vault,
        username_secret: &str,
        hash_secret: &str,
    ) -> Result<Self, AuthError> {
        let read = |name: &str| {
            vault
                .resolve(name)
                .map(|s| s.expose().to_string())
                .map_err(|e| AuthError(format!("login secret: {e}")))
        };
        let username = read(username_secret)?;
        let hash = read(hash_secret)?;
        PasswordHash::new(&hash)
            .map_err(|_| AuthError(format!("`{hash_secret}` is not an argon2 password hash")))?;
        Ok(Self {
            shared: Arc::new(Shared {
                username,
                hash,
                sessions: Mutex::new(HashMap::new()),
            }),
            lifetime: Duration::from_secs(12 * 3600),
            secure_cookie: true,
        })
    }

    /// How long a session lasts (default 12 hours).
    pub fn session_lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Whether the cookie carries `Secure`, so browsers send it only over HTTPS
    /// (default true; turn it off only for plain-http local use).
    pub fn secure_cookie(mut self, secure: bool) -> Self {
        self.secure_cookie = secure;
        self
    }

    fn check(&self, username: &str, password: &str) -> bool {
        // Always verify, so a wrong username takes as long as a wrong password.
        let hash = PasswordHash::new(&self.shared.hash).expect("checked at construction");
        let password_ok = Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok();
        username == self.shared.username && password_ok
    }

    fn open_session(&self) -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let mut sessions = mutex(&self.shared.sessions);
        let now = Instant::now();
        sessions.retain(|_, expires| *expires > now);
        sessions.insert(token.clone(), now + self.lifetime);
        token
    }

    fn valid(&self, token: &str) -> bool {
        let mut sessions = mutex(&self.shared.sessions);
        match sessions.get(token) {
            Some(expires) if *expires > Instant::now() => true,
            Some(_) => {
                sessions.remove(token);
                false
            }
            None => false,
        }
    }

    fn close_session(&self, token: &str) {
        mutex(&self.shared.sessions).remove(token);
    }

    fn cookie(&self, value: &str, max_age: u64) -> HeaderValue {
        let secure = if self.secure_cookie { "; Secure" } else { "" };
        HeaderValue::from_str(&format!(
            "{COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}"
        ))
        .expect("a hex token is a valid header value")
    }

    /// Wraps `router` so that every route needs a session, and adds the login
    /// routes (`GET|POST /login`, `POST /logout`).
    pub(crate) fn protect(&self, router: Router) -> Router {
        let login = Router::new()
            .route("/login", get(login_page).post(login_submit))
            .route("/logout", post(logout))
            .with_state(self.clone());
        router
            .merge(login)
            .layer(axum::middleware::from_fn_with_state(
                self.clone(),
                require_session,
            ))
    }
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let value = value.to_str().ok()?;
        return value
            .strip_prefix("Bearer ")
            .filter(|t| !t.is_empty())
            .map(String::from);
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(&format!("{COOKIE}=")))
        .map(String::from)
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn require_session(State(auth): State<Auth>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if path == "/login" {
        return next.run(request).await;
    }
    if session_token(request.headers()).is_some_and(|t| auth.valid(&t)) {
        return next.run(request).await;
    }
    if path.starts_with("/api/") || path == "/api" || path == "/logout" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "login required"})),
        )
            .into_response();
    }
    let wanted = request
        .uri()
        .path_and_query()
        .map_or("/", |p| p.as_str())
        .to_string();
    Redirect::to(&format!("/login?next={}", percent_encode(&wanted))).into_response()
}

async fn login_page() -> Html<&'static str> {
    Html(LOGIN_PAGE)
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

async fn login_submit(State(auth): State<Auth>, Json(c): Json<Credentials>) -> Response {
    let checker = auth.clone();
    let ok = tokio::task::spawn_blocking(move || checker.check(&c.username, &c.password))
        .await
        .unwrap_or(false);
    if !ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "wrong username or password"})),
        )
            .into_response();
    }
    let token = auth.open_session();
    let mut response = Json(json!({"token": token})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        auth.cookie(&token, auth.lifetime.as_secs()),
    );
    response
}

async fn logout(State(auth): State<Auth>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        auth.close_session(&token);
    }
    let mut response = Json(json!({"ok": true})).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, auth.cookie("", 0));
    response
}
