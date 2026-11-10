//! KR-07: auth is required by default. Building an Agora without a login
//! — and without an explicit opt-out — must fail, so a forgotten `.auth()`
//! can never leave the router open.

use archodin_console::{hash_password, Agora, Auth};
use archodin_graph::Vault;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

async fn get(app: &Router, uri: &str) -> StatusCode {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    res.status()
}

fn auth() -> Auth {
    let vault = Vault::new([9; 32]);
    vault.put("agora/username", "kudo");
    vault.put(
        "agora/password_hash",
        &hash_password("correct horse battery staple"),
    );
    Auth::from_vault(&vault, "agora/username", "agora/password_hash").unwrap()
}

#[test]
#[should_panic(expected = "needs a login")]
fn building_without_auth_or_an_explicit_opt_out_panics() {
    let _agora = Agora::builder().runner(|_| Box::pin(async {})).build();
}

#[tokio::test]
async fn an_explicit_opt_out_leaves_the_router_open() {
    let app = Agora::builder()
        .runner(|_| Box::pin(async {}))
        .insecure_no_auth()
        .build()
        .router();
    assert_eq!(get(&app, "/api/jobs").await, StatusCode::OK);
    assert_eq!(get(&app, "/").await, StatusCode::OK);
}

#[tokio::test]
async fn auth_still_protects_every_route_when_set() {
    let app = Agora::builder()
        .runner(|_| Box::pin(async {}))
        .auth(auth())
        .build()
        .router();
    assert_eq!(get(&app, "/api/jobs").await, StatusCode::UNAUTHORIZED);
    assert_eq!(get(&app, "/api/usage").await, StatusCode::UNAUTHORIZED);
    assert_eq!(get(&app, "/").await, StatusCode::SEE_OTHER);
}
