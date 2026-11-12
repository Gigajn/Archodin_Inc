//! Login for the Agora: only the person who holds the password can open it.
//!
//! The username and the argon2 password hash live in the [`Vault`], never in plain
//! configuration ([`Auth::from_vault`]). A correct login creates a session; the
//! session is carried by an `HttpOnly` cookie or, for scripts, by an
//! `Authorization: Bearer` token (both come from `POST /login`). A middleware layer
//! puts every route, including unknown paths, behind the session.
//!
//! Brute force is slowed down in two ways. Argon2 makes each guess expensive,
//! and failed logins are counted per (client IP, username): after
//! [`DEFAULT_MAX_FAILED_LOGINS`] failures the key is locked out, with the lockout
//! doubling on every further failure (capped at [`DEFAULT_MAX_LOCKOUT`]). A
//! successful login clears the count. Failed attempts and lockouts are logged to
//! stderr (username and client only, never the password).
//!
//! The client is the socket peer ([`ConnectInfo`]) when the server was started
//! with `into_make_service_with_connect_info`; otherwise it is `"unknown"` and
//! the limit still applies per username. `X-Forwarded-For` is deliberately NOT
//! trusted: anyone can send it, so it would let an attacker rotate identities
//! and dodge the limit.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use archodin_graph::locks::mutex;
use archodin_graph::Vault;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, Extension, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

const COOKIE: &str = "agora_session";
const LOGIN_PAGE: &str = include_str!("login.html");

/// Failed logins allowed per (client, username) before a lockout.
pub const DEFAULT_MAX_FAILED_LOGINS: u32 = 5;
/// Lockout after the first [`DEFAULT_MAX_FAILED_LOGINS`] failures; it doubles
/// on every further failure.
pub const DEFAULT_LOCKOUT_BASE: Duration = Duration::from_secs(30);
/// The longest any lockout lasts.
pub const DEFAULT_MAX_LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// How long a failed-login record is kept after its last attempt (for the
/// backoff to keep growing across lockouts); stale records are pruned.
const ATTEMPT_MEMORY: Duration = Duration::from_secs(3600);

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

/// One key the login limit counts: (client, username).
type AttemptKey = (String, String);

struct Attempt {
    failures: u32,
    locked_until: Option<Instant>,
    last_seen: Instant,
}

struct Shared {
    username: String,
    hash: String,
    sessions: Mutex<HashMap<String, Instant>>,
    attempts: Mutex<HashMap<AttemptKey, Attempt>>,
}

/// The Agora's login. Clones share the same sessions and the same failed-login
/// counts.
#[derive(Clone)]
pub struct Auth {
    shared: Arc<Shared>,
    lifetime: Duration,
    secure_cookie: bool,
    max_failed_logins: u32,
    lockout_base: Duration,
    max_lockout: Duration,
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
                attempts: Mutex::new(HashMap::new()),
            }),
            lifetime: Duration::from_secs(12 * 3600),
            secure_cookie: true,
            max_failed_logins: DEFAULT_MAX_FAILED_LOGINS,
            lockout_base: DEFAULT_LOCKOUT_BASE,
            max_lockout: DEFAULT_MAX_LOCKOUT,
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

    /// Failed logins allowed per (client, username) before a lockout
    /// (default [`DEFAULT_MAX_FAILED_LOGINS`]).
    pub fn failed_login_limit(mut self, n: u32) -> Self {
        self.max_failed_logins = n.max(1);
        self
    }

    /// Lockout after the first failures; it doubles on every further failure
    /// up to the max lockout (default [`DEFAULT_LOCKOUT_BASE`]). A base longer
    /// than the max lockout is silently clamped to it; see
    /// [`Auth::max_lockout`].
    pub fn lockout_base(mut self, base: Duration) -> Self {
        self.lockout_base = base;
        self
    }

    /// The longest any lockout lasts (default [`DEFAULT_MAX_LOCKOUT`]).
    /// A [`lockout_base`](Self::lockout_base) longer than this is silently
    /// clamped to it. Clamped to at least one second.
    pub fn max_lockout(mut self, max: Duration) -> Self {
        self.max_lockout = max.max(Duration::from_secs(1));
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

    /// Whether this (client, username) may try to log in right now. Also
    /// prunes stale failed-login records so the map stays bounded.
    fn login_allowed(&self, key: &AttemptKey) -> bool {
        let now = Instant::now();
        let mut attempts = mutex(&self.shared.attempts);
        attempts.retain(|_, a| {
            a.locked_until.is_some_and(|until| until > now)
                || now.duration_since(a.last_seen) < ATTEMPT_MEMORY
        });
        !matches!(
            attempts.get(key),
            Some(a) if a.locked_until.is_some_and(|until| until > now)
        )
    }

    /// How long the lockout lasts for this many consecutive failures.
    fn lockout_duration(&self, failures: u32) -> Duration {
        let extra = failures.saturating_sub(self.max_failed_logins);
        let mut secs = self.lockout_base.as_secs().max(1);
        for _ in 0..extra.min(10) {
            secs = secs.saturating_mul(2);
        }
        Duration::from_secs(secs.min(self.max_lockout.as_secs()))
    }

    /// Records a failed login. Returns true if this failure tripped (or
    /// re-tripped) a lockout. Logs the username and client, never the password.
    fn record_failed_login(&self, key: AttemptKey) -> bool {
        let mut attempts = mutex(&self.shared.attempts);
        let now = Instant::now();
        let entry = attempts.entry(key.clone()).or_insert_with(|| Attempt {
            failures: 0,
            locked_until: None,
            last_seen: now,
        });
        entry.failures += 1;
        entry.last_seen = now;
        if entry.failures >= self.max_failed_logins {
            let wait = self.lockout_duration(entry.failures);
            entry.locked_until = Some(now + wait);
            eprintln!(
                "[agora] login locked out: user `{}` from {} after {} failed attempts; retry in {}s",
                key.1,
                key.0,
                entry.failures,
                wait.as_secs()
            );
            true
        } else {
            eprintln!(
                "[agora] failed login: user `{}` from {} (attempt {} of {})",
                key.1, key.0, entry.failures, self.max_failed_logins
            );
            false
        }
    }

    /// A successful login clears the failed-login count for this key.
    fn record_successful_login(&self, key: &AttemptKey) {
        mutex(&self.shared.attempts).remove(key);
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

async fn login_submit(
    State(auth): State<Auth>,
    // `Option<ConnectInfo<_>>` does NOT work in axum 0.8: `ConnectInfo`
    // lacks `OptionalFromRequestParts`, so the `Handler` bound fails.
    // `Extension<_>` does implement it, so the optional form is
    // `Option<Extension<ConnectInfo<_>>>` — `None` when the server was
    // not started with `into_make_service_with_connect_info` (tests).
    connect: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(c): Json<Credentials>,
) -> Response {
    let client = connect
        .map(|Extension(ConnectInfo(addr))| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let key: AttemptKey = (client, c.username.clone());
    if !auth.login_allowed(&key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "too many failed logins, try again later"})),
        )
            .into_response();
    }
    let checker = auth.clone();
    let (username, password) = (c.username, c.password);
    let ok = tokio::task::spawn_blocking(move || checker.check(&username, &password))
        .await
        .unwrap_or(false);
    if !ok {
        let locked_out = auth.record_failed_login(key);
        let (status, error) = if locked_out {
            (
                StatusCode::TOO_MANY_REQUESTS,
                "too many failed logins, try again later",
            )
        } else {
            (StatusCode::UNAUTHORIZED, "wrong username or password")
        };
        return (status, Json(json!({"error": error}))).into_response();
    }
    auth.record_successful_login(&key);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_with(base: Duration, max: Duration) -> Auth {
        let vault = Vault::new([0; 32].into());
        vault.put("u", "kudo");
        vault.put("h", &hash_password("pw"));
        Auth::from_vault(&vault, "u", "h")
            .unwrap()
            .lockout_base(base)
            .max_lockout(max)
    }

    #[test]
    fn lockout_doubles_each_failure_and_stops_at_the_cap() {
        let auth = auth_with(Duration::from_secs(2), Duration::from_secs(15 * 60));
        // The 5th failure (the limit) trips the base lockout; each failure
        // after that doubles it until the cap binds.
        assert_eq!(auth.lockout_duration(5), Duration::from_secs(2));
        assert_eq!(auth.lockout_duration(6), Duration::from_secs(4));
        assert_eq!(auth.lockout_duration(7), Duration::from_secs(8));
        assert_eq!(auth.lockout_duration(100), Duration::from_secs(15 * 60));
    }

    #[test]
    fn lockout_base_above_the_max_is_clamped() {
        let auth = auth_with(Duration::from_secs(3600), Duration::from_secs(60));
        assert_eq!(auth.lockout_duration(5), Duration::from_secs(60));
    }
}
