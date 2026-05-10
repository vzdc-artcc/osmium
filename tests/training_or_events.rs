mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, env_test_lock, json_body};

#[tokio::test(flavor = "current_thread")]
async fn event_staff_flow_works_end_to_end() {
    let _env_lock = env_test_lock().lock().unwrap();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000041,
            "Event Staff",
            &[
                "events.items.create",
                "events.positions.assign",
                "events.positions.publish",
            ],
        )
        .await;
    let user = app
        .create_user(10000042, "Event User", &["events.positions.self.request"])
        .await;

    let create_event_response = app
        .json_request(
            "POST",
            "/api/v1/events",
            Some(&staff.session_token),
            Some(json!({
                "title": "Integration Event",
                "event_type": "STANDARD",
                "host": "vZDC",
                "description": "event workflow integration test",
                "starts_at": "2026-05-09T15:00:00Z",
                "ends_at": "2026-05-09T17:00:00Z"
            })),
        )
        .await;
    assert_status(&create_event_response, StatusCode::CREATED);
    let event_body: Value = json_body(create_event_response).await;
    let event_id = event_body["id"].as_str().unwrap().to_string();

    let request_position_response = app
        .json_request(
            "POST",
            &format!("/api/v1/events/{event_id}/positions"),
            Some(&user.session_token),
            Some(json!({
                "callsign": "DCA_DEL",
                "requested_slot": 1
            })),
        )
        .await;
    assert_status(&request_position_response, StatusCode::CREATED);
    let position_body: Value = json_body(request_position_response).await;
    let position_id = position_body["id"].as_str().unwrap().to_string();
    assert_eq!(position_body["status"], "REQUESTED");

    let assign_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/events/{event_id}/positions/{position_id}"),
            Some(&staff.session_token),
            Some(json!({
                "user_id": user.id,
                "assigned_slot": 1
            })),
        )
        .await;
    assert_status(&assign_response, StatusCode::OK);
    let assign_body: Value = json_body(assign_response).await;
    assert_eq!(assign_body["status"], "ASSIGNED");

    let publish_response = app
        .json_request(
            "POST",
            &format!("/api/v1/events/{event_id}/positions/publish"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&publish_response, StatusCode::OK);

    let list_response = app
        .json_request(
            "GET",
            &format!("/api/v1/events/{event_id}/positions"),
            None,
            None,
        )
        .await;
    assert_status(&list_response, StatusCode::OK);
    let list_body: Value = json_body(list_response).await;
    assert_eq!(list_body["items"][0]["published"], true);
    assert_eq!(list_body["items"][0]["user_id"], user.id);

    app.cleanup().await;
}
