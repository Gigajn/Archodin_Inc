//! The Agora model API (ADR 0020 §Decision 3, ADR 0024) through HTTP,
//! driven in-process (no network ports) — plus the canary test
//! (ADR 0025 §1): a recognisable fake key must never appear in an API
//! response, an audit row, a `Debug` dump, or an error message.

use std::sync::{Arc, Mutex};

use archodin_console::{hash_password, Agora, Auth, JobContext, ModelApi};
use archodin_graph::{Registry, UsageMeter, Vault};
use archodin_store::{MemoryStore, SealedVaultStore};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// `sk-canary-*` placeholders only — never a real key (ADR 0021).
const CANARY: &str = "sk-canary-7f3a9c-main";
const CANARY2: &str = "sk-canary-7f3a9c-rotated";

struct Fixture {
    app: Router,
    store: Arc<MemoryStore>,
    vault: Vault,
    registry: Arc<Mutex<Registry>>,
    api: ModelApi,
}

fn fixture(vault_key_configured: bool, vault_locked: bool) -> Fixture {
    let vault = Vault::new([7; 32].into());
    let registry = Arc::new(Mutex::new(Registry::new()));
    let store = Arc::new(MemoryStore::new());
    let api = ModelApi::new(
        vault.clone(),
        Arc::clone(&registry),
        Arc::clone(&store) as Arc<dyn archodin_store::ModelStore + Send + Sync>,
        Arc::new(UsageMeter::new()),
        vault_key_configured,
        vault_locked,
    );
    let app = Agora::builder()
        .runner(|_: JobContext| Box::pin(async {}))
        .insecure_no_auth()
        .model_api(api.clone())
        .build()
        .router();
    Fixture {
        app,
        store,
        vault,
        registry,
        api,
    }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn json_of(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, text) = call(app, method, uri, body, None).await;
    let value: Value = serde_json::from_str(&text)
        .unwrap_or_else(|_| panic!("{method} {uri} -> {status}: not JSON: {text}"));
    (status, value)
}

fn provision_body(name: &str, kind: &str, base_url: &str, key: &str) -> Value {
    json!({
        "name": name,
        "kind": kind,
        "base_url": base_url,
        "model": "m1",
        "key": key,
    })
}

#[tokio::test]
async fn a_canary_key_never_appears_in_any_api_surface() {
    let mut server = mockito::Server::new_async().await;
    let base = server.url();
    let f = fixture(false, false);

    // Provision: degraded mode (no Vault key) — in-memory only, and the
    // response says so explicitly instead of pretending to persist.
    let (status, created) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body("main", "openai_compatible", &base, CANARY)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["persisted"], false);
    assert!(created["persist_reason"]
        .as_str()
        .unwrap()
        .contains("BASILEIA_VAULT_KEY"));
    assert_eq!(created["account"]["name"], "main");
    assert_eq!(created["account"]["status"], "ok");
    // The sealed blob was NOT written in degraded mode.
    assert!(f.store.load_sealed_vault().await.unwrap().is_none());
    // The key did land in the Vault (the shared handle proves it).
    assert_eq!(
        f.vault
            .resolve("model/openai_compatible/main")
            .unwrap()
            .expose(),
        CANARY
    );

    // The list and the status never carry the key.
    let (status, list) = json_of(&f.app, "GET", "/api/models/accounts", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["accounts"][0]["name"], "main");
    assert_eq!(list["accounts"][0]["status"], "ok");
    assert_eq!(list["vault_locked"], false);
    let (_, status_body) = json_of(&f.app, "GET", "/api/models/status", None).await;
    assert_eq!(status_body["vault_key_configured"], false);
    for text in [
        serde_json::to_string(&created).unwrap(),
        serde_json::to_string(&list).unwrap(),
        serde_json::to_string(&status_body).unwrap(),
    ] {
        assert!(!text.contains(CANARY), "{text}");
    }
    // Neither does any Debug dump of the state that holds it.
    assert!(!format!("{:?}", f.vault).contains(CANARY));
    assert!(!format!("{:?}", f.api).contains(CANARY));
    assert!(!format!("{:?}", f.registry.lock().unwrap()).contains(CANARY));

    // Rotate: the new key replaces the old atomically, and the response
    // still never shows either value.
    let (status, rotated) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts/main/rotate",
        Some(json!({ "key": CANARY2 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rotated}");
    assert_eq!(rotated["persisted"], false);
    assert_eq!(
        f.vault
            .resolve("model/openai_compatible/main")
            .unwrap()
            .expose(),
        CANARY2
    );
    assert!(!serde_json::to_string(&rotated).unwrap().contains(CANARY2));
    assert!(!serde_json::to_string(&rotated).unwrap().contains(CANARY));

    // Health: 200 → ok, then 401 → auth_failed (the raw error body,
    // which echoes the key, is never returned).
    let ok = server.mock("GET", "/v1/models").with_status(200).create();
    let (status, health) = json_of(&f.app, "POST", "/api/models/accounts/main/health", None).await;
    ok.assert();
    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["status"], "ok");

    let denied = server
        .mock("GET", "/v1/models")
        .with_status(401)
        .with_body(format!("{{\"error\": \"bad key {CANARY2}\"}}"))
        .create();
    let (status, health) = json_of(&f.app, "POST", "/api/models/accounts/main/health", None).await;
    denied.assert();
    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["status"], "auth_failed");
    assert!(!serde_json::to_string(&health).unwrap().contains(CANARY2));

    // A dead server → unreachable.
    let (_, _) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body(
            "dead",
            "openai_compatible",
            "http://127.0.0.1:9",
            CANARY,
        )),
    )
    .await;
    let (status, health) = json_of(&f.app, "POST", "/api/models/accounts/dead/health", None).await;
    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["status"], "unreachable");
    assert!(!serde_json::to_string(&health).unwrap().contains(CANARY));

    // The audit log records the lifecycle (newest first) — and no row
    // carries either canary, in any field.
    let (status, audit) = json_of(&f.app, "GET", "/api/models/audit", None).await;
    assert_eq!(status, StatusCode::OK);
    let actions: Vec<&str> = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            "health_check",
            "provision",
            "auth_failed",
            "health_check",
            "rotate",
            "provision",
        ],
        "{audit}"
    );
    let audit_text = serde_json::to_string(&audit).unwrap();
    assert!(!audit_text.contains(CANARY), "{audit_text}");
    assert!(!audit_text.contains(CANARY2), "{audit_text}");

    // Revoke = delete: the key leaves the Vault, the account leaves the
    // list, and later calls fail with a clear error, not a hang.
    let (status, deleted) = json_of(&f.app, "DELETE", "/api/models/accounts/main", None).await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["revoked"], "main");
    assert!(f.vault.resolve("model/openai_compatible/main").is_err());
    let (_, list) = json_of(&f.app, "GET", "/api/models/accounts", None).await;
    assert_eq!(list["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(list["accounts"][0]["name"], "dead");
    let (status, body) = call(
        &f.app,
        "POST",
        "/api/models/accounts/main/health",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (_, audit) = json_of(&f.app, "GET", "/api/models/audit", None).await;
    assert_eq!(audit["events"][0]["action"], "revoke");
    assert!(!serde_json::to_string(&audit).unwrap().contains(CANARY));
    assert!(!serde_json::to_string(&audit).unwrap().contains(CANARY2));
}

#[tokio::test]
async fn provision_persists_the_sealed_blob_when_a_vault_key_is_configured() {
    let f = fixture(true, false);
    let (status, created) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body(
            "main",
            "claude",
            "https://api.anthropic.com",
            CANARY,
        )),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["persisted"], true);
    assert!(created.get("persist_reason").is_none());

    // The blob reopens with the same Vault key, and holds the key.
    let blob = f.store.load_sealed_vault().await.unwrap().unwrap();
    assert!(!String::from_utf8_lossy(&blob).contains(CANARY));
    let reopened = Vault::unseal_envelope([7; 32].into(), &blob).unwrap();
    assert_eq!(
        reopened.resolve("model/claude/main").unwrap().expose(),
        CANARY
    );

    // Rotate re-seals: the blob now holds the new key, not the old one.
    let (status, _) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts/main/rotate",
        Some(json!({ "key": CANARY2 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let blob = f.store.load_sealed_vault().await.unwrap().unwrap();
    let reopened = Vault::unseal_envelope([7; 32].into(), &blob).unwrap();
    assert_eq!(
        reopened.resolve("model/claude/main").unwrap().expose(),
        CANARY2
    );

    // Revoke re-seals too: the secret is gone from the reopened Vault.
    let (status, _) = json_of(&f.app, "DELETE", "/api/models/accounts/main", None).await;
    assert_eq!(status, StatusCode::OK);
    let blob = f.store.load_sealed_vault().await.unwrap().unwrap();
    let reopened = Vault::unseal_envelope([7; 32].into(), &blob).unwrap();
    assert!(reopened.resolve("model/claude/main").is_err());
}

#[tokio::test]
async fn provision_validation_refuses_bad_shapes() {
    let f = fixture(false, false);
    let base = "https://api.example.com/v1";
    for (name, kind, key) in [
        ("Main", "claude", CANARY),       // uppercase slug
        ("my-account", "claude", CANARY), // dash, not a slug
        ("", "claude", CANARY),           // empty name
        ("main", "bedrock", CANARY),      // unknown kind
        ("main", "claude", ""),           // empty key
        ("main", "claude", "sk-foo bar"), // whitespace in the key
    ] {
        // Error bodies are plain text (the Agora's ApiError shape).
        let (status, body) = call(
            &f.app,
            "POST",
            "/api/models/accounts",
            Some(provision_body(name, kind, base, key)),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name}/{kind}: {body}");
        assert!(!body.contains(CANARY), "{body}");
    }
    // Ollama needs no key — and must not carry one.
    let (status, body) = call(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(json!({
            "name": "local", "kind": "ollama",
            "base_url": "http://127.0.0.1:11434", "model": "llama3.2",
            "key": CANARY,
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(!body.contains(CANARY), "{body}");
    let (status, created) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(json!({
            "name": "local", "kind": "ollama",
            "base_url": "http://127.0.0.1:11434", "model": "llama3.2",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    // A bad base URL is refused, naming the problem, not the key.
    let (status, body) = call(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body("bad", "claude", "not a url", CANARY)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(!body.contains(CANARY), "{body}");
    // Unknown accounts: 404 on every per-account route.
    for (method, uri) in [
        ("POST", "/api/models/accounts/ghost/rotate"),
        ("POST", "/api/models/accounts/ghost/health"),
        ("DELETE", "/api/models/accounts/ghost"),
        ("GET", "/api/models/accounts/ghost/budget"),
    ] {
        let body = if method == "POST" {
            Some(json!({ "key": CANARY }))
        } else {
            None
        };
        let (status, _) = call(&f.app, method, uri, body, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[tokio::test]
async fn health_check_with_a_missing_key_is_a_conflict_not_a_hang() {
    let f = fixture(false, false);
    // Provision, then drop the key from the Vault directly (as a revoke
    // without the Registry half would): the next health check reports
    // the missing key instead of calling the provider.
    let (_, _) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body(
            "main",
            "claude",
            "https://api.anthropic.com",
            CANARY,
        )),
    )
    .await;
    f.vault.remove("model/claude/main");
    let (status, body) = call(
        &f.app,
        "POST",
        "/api/models/accounts/main/health",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("is missing"), "{body}");
    assert!(!body.contains(CANARY), "{body}");
    // The list now shows the account as locked.
    let (_, list) = json_of(&f.app, "GET", "/api/models/accounts", None).await;
    assert_eq!(list["accounts"][0]["status"], "locked");
    assert_eq!(list["locked_accounts"][0], "main");
}

#[tokio::test]
async fn budget_endpoints_set_report_and_clear_caps() {
    let f = fixture(false, false);
    let (_, _) = json_of(
        &f.app,
        "POST",
        "/api/models/accounts",
        Some(provision_body(
            "main",
            "openai_compatible",
            "https://api.example.com/v1",
            CANARY,
        )),
    )
    .await;
    // No cap yet: unmeasured, nothing used, nothing exceeded.
    let (status, budget) = json_of(&f.app, "GET", "/api/models/accounts/main/budget", None).await;
    assert_eq!(status, StatusCode::OK, "{budget}");
    assert_eq!(budget["budget_tokens"], Value::Null);
    assert_eq!(budget["measured"], "unmeasured");
    assert_eq!(budget["used_tokens"], 0);
    assert_eq!(budget["exceeded"], false);

    let (status, budget) = json_of(
        &f.app,
        "PUT",
        "/api/models/accounts/main/budget",
        Some(json!({ "budget_tokens": 100, "budget_cost": 2.5 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{budget}");
    assert_eq!(budget["budget_tokens"], 100);
    assert_eq!(budget["budget_cost"], 2.5);
    // Still unmeasured: a cap is not usage.
    assert_eq!(budget["measured"], "unmeasured");

    // Negative and non-finite caps are refused.
    for body in [
        json!({ "budget_tokens": -1 }),
        json!({ "budget_cost": -0.5 }),
    ] {
        let (status, _) = call(
            &f.app,
            "PUT",
            "/api/models/accounts/main/budget",
            Some(body),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // Clearing both caps persists as nulls.
    let (status, budget) = json_of(
        &f.app,
        "PUT",
        "/api/models/accounts/main/budget",
        Some(json!({ "budget_tokens": null, "budget_cost": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{budget}");
    assert_eq!(budget["budget_tokens"], Value::Null);
    assert_eq!(budget["budget_cost"], Value::Null);
    // ...and the list reflects the cleared caps.
    let (_, list) = json_of(&f.app, "GET", "/api/models/accounts", None).await;
    assert_eq!(list["accounts"][0]["budget_tokens"], Value::Null);
}

#[tokio::test]
async fn model_routes_are_behind_the_admin_login() {
    let vault = Vault::new([7; 32].into());
    vault.put("agora/username", "admin");
    vault.put("agora/password_hash", &hash_password("pw"));
    let auth = Auth::from_vault(&vault, "agora/username", "agora/password_hash").unwrap();
    let registry = Arc::new(Mutex::new(Registry::new()));
    let store = Arc::new(MemoryStore::new());
    let api = ModelApi::new(
        vault,
        Arc::clone(&registry),
        Arc::clone(&store) as Arc<dyn archodin_store::ModelStore + Send + Sync>,
        Arc::new(UsageMeter::new()),
        false,
        false,
    );
    let app = Agora::builder()
        .runner(|_: JobContext| Box::pin(async {}))
        .auth(auth)
        .model_api(api)
        .build()
        .router();

    // No session: 401 on the model API, exactly like the other routes.
    let (status, _) = call(&app, "GET", "/api/models/status", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(&app, "GET", "/api/models/accounts", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Log in, then the same routes answer with the session token.
    let (status, login) = json_of(
        &app,
        "POST",
        "/login",
        Some(json!({ "username": "admin", "password": "pw" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login}");
    let token = login["token"].as_str().unwrap();
    let (status, _) = call(&app, "GET", "/api/models/status", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "GET", "/api/models/accounts", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn model_routes_answer_503_without_a_model_api() {
    let app = Agora::builder()
        .runner(|_: JobContext| Box::pin(async {}))
        .insecure_no_auth()
        .build()
        .router();
    let (status, _) = call(&app, "GET", "/api/models/status", None, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = call(&app, "GET", "/api/models/accounts", None, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    // The rest of the Agora still works without the model API.
    let (status, _) = call(&app, "GET", "/api/approvals", None, None).await;
    assert_eq!(status, StatusCode::OK);
}
