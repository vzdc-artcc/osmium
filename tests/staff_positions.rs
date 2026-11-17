mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn admin_can_assign_the_legacy_ata_position() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000500,
            "Position Editor",
            &["users.staff_positions.update"],
        )
        .await;
    let target = app.create_user(10000501, "Assistant TA", &[]).await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/staff-positions/ATA", target.cid),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let positions: Vec<&str> = body["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["position"].as_str().unwrap())
        .collect();
    assert_eq!(positions, vec!["ATA"]);
    assert_eq!(body["positions"][0]["source"], "manual");

    let response = app
        .json_request("GET", "/api/v1/staff-positions/ATA/holders", None, None)
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["holders"][0]["cid"], target.cid);

    app.cleanup().await;
}
