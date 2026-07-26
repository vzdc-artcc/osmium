mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{TestApp, assert_status, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn create_feedback_rejects_self_submission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000051, "Self Submitter", &["feedback.items.create"])
        .await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&submitter.session_token),
            Some(json!({
                "target_cid": submitter.cid,
                "pilot_callsign": "DAL123",
                "controller_position": "DCA_DEL",
                "rating": 5,
                "comments": "great job"
            })),
        )
        .await;

    assert_status(&response, StatusCode::BAD_REQUEST);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn create_feedback_succeeds_for_different_target() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000052, "Feedback Submitter", &["feedback.items.create"])
        .await;
    let target = app.create_user(10000053, "Feedback Target", &[]).await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&submitter.session_token),
            Some(json!({
                "target_cid": target.cid,
                "pilot_callsign": "DAL123",
                "controller_position": "DCA_DEL",
                "rating": 5,
                "comments": "great job"
            })),
        )
        .await;

    assert_status(&response, StatusCode::CREATED);

    app.cleanup().await;
}
