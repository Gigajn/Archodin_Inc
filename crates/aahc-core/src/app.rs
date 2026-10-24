//! HTTP application surface.

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde::Serialize;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Build the office HTTP application.
pub fn app() -> Router {
    Router::new().route("/health", get(health))
}
