mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

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

#[tokio::test(flavor = "current_thread")]
async fn list_feedback_requires_a_read_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let nobody = app.create_user(10000054, "No Feedback Access", &[]).await;

    let response = app
        .json_request("GET", "/api/v1/feedback", Some(&nobody.session_token), None)
        .await;

    assert_status(&response, StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn list_feedback_with_self_permission_returns_only_own_submissions() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(
            10000055,
            "Self List Submitter",
            &["feedback.items.create", "feedback.items_self.read"],
        )
        .await;
    let other_submitter = app
        .create_user(10000056, "Other Submitter", &["feedback.items.create"])
        .await;
    let target = app.create_user(10000057, "Shared Target", &[]).await;

    let own_response = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&submitter.session_token),
            Some(json!({
                "target_cid": target.cid,
                "pilot_callsign": "DAL100",
                "controller_position": "DCA_DEL",
                "rating": 5,
                "comments": "own submission"
            })),
        )
        .await;
    assert_status(&own_response, StatusCode::CREATED);

    let other_response = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&other_submitter.session_token),
            Some(json!({
                "target_cid": target.cid,
                "pilot_callsign": "DAL200",
                "controller_position": "DCA_DEL",
                "rating": 4,
                "comments": "someone else's submission"
            })),
        )
        .await;
    assert_status(&other_response, StatusCode::CREATED);

    let list_response = app
        .json_request(
            "GET",
            "/api/v1/feedback",
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&list_response, StatusCode::OK);
    let list_body: Value = json_body(list_response).await;
    let items = list_body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["comments"], "own submission");

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn get_user_feedback_allows_self_view_with_self_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000058, "Other Submitter", &["feedback.items.create"])
        .await;
    let target = app
        .create_user(10000059, "Self Viewer", &["feedback.items_self.read"])
        .await;

    let submit_response = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&submitter.session_token),
            Some(json!({
                "target_cid": target.cid,
                "pilot_callsign": "DAL300",
                "controller_position": "DCA_DEL",
                "rating": 5,
                "comments": "feedback about the target"
            })),
        )
        .await;
    assert_status(&submit_response, StatusCode::CREATED);

    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/users/{}/feedback", target.cid),
            Some(&target.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    app.cleanup().await;
}
