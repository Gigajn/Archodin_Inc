//! Seam: the public HTTP surface. Behaviour only - no peeking inside.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

async fn get_health() -> (StatusCode, Value) {
    let response = aahc_core::app::app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible");

    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn health_reports_the_service_is_ok() {
    let (status, body) = get_health().await;

    assert_eq!(
        status,
        StatusCode::OK,
        "health must answer 200, got {status}"
    );
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn health_reports_the_service_version() {
    let (status, body) = get_health().await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn unknown_route_is_not_found() {
    let response = aahc_core::app::app()
        .oneshot(
            Request::builder()
                .uri("/definitely-not-a-route")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
