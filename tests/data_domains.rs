//! Tests for Worker B's new data domains (spec 012): the GDPR self-service data
//! export and the FAA preferred-routes search endpoint.
//!
//! These are DB-less router/OpenAPI checks — their point is to prove the two new
//! routes are wired and are NOT accidentally public. The full data-assembly paths
//! are exercised by the DB-backed suite the review worker runs against Postgres.

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn preferred_routes_requires_authentication() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/routes/preferred?origin=KJFK")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Reference data, but deliberately not public: no session => 401, never 200.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn data_export_requires_authentication() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/me/data-export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Self-service export must never be reachable without a session.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn new_data_domain_routes_are_in_openapi() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/docs/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let paths = json
        .get("paths")
        .and_then(|value| value.as_object())
        .unwrap();

    for expected in ["/api/v1/routes/preferred", "/api/v1/me/data-export"] {
        assert!(
            paths.contains_key(expected),
            "missing OpenAPI path: {expected}"
        );
    }
}
