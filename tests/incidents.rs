mod support;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::json;
use support::{TestApp, assert_status, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn create_incident_rejects_self_submission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reporter = app
        .create_user(10000061, "Self Reporter", &["feedback.items.create"])
        .await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/incidents",
            Some(&reporter.session_token),
            Some(json!({
                "reportee_cid": reporter.cid,
                "timestamp": Utc::now().to_rfc3339(),
                "reason": "self reported incident",
                "reportee_callsign": "DAL123"
            })),
        )
        .await;

    assert_status(&response, StatusCode::BAD_REQUEST);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn create_incident_succeeds_for_different_reportee() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reporter = app
        .create_user(10000062, "Incident Reporter", &["feedback.items.create"])
        .await;
    let reportee = app.create_user(10000063, "Incident Reportee", &[]).await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/incidents",
            Some(&reporter.session_token),
            Some(json!({
                "reportee_cid": reportee.cid,
                "timestamp": Utc::now().to_rfc3339(),
                "reason": "incident report",
                "reportee_callsign": "DAL123"
            })),
        )
        .await;

    assert_status(&response, StatusCode::CREATED);

    app.cleanup().await;
}
