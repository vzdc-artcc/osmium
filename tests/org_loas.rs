mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn loa_self_service_and_admin_lifecycle_works_end_to_end() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(
            10000097,
            "Loa Submitter",
            &["auth.profile.read", "auth.profile.update"],
        )
        .await;
    let staff = app
        .create_user(
            10000098,
            "Loa Staff",
            &["users.directory.read", "users.controller_status.update"],
        )
        .await;
    let other_user = app
        .create_user(
            10000099,
            "Loa Other",
            &["auth.profile.read", "auth.profile.update"],
        )
        .await;

    let start = Utc::now() + Duration::days(10);
    let end = start + Duration::days(10);

    // Too-short duration is rejected (backend minimum is 7 days).
    let too_short = app
        .json_request(
            "POST",
            "/api/v1/loa/me",
            Some(&submitter.session_token),
            Some(json!({
                "start": start.to_rfc3339(),
                "end": (start + Duration::days(2)).to_rfc3339(),
                "reason": "Too short"
            })),
        )
        .await;
    assert_status(&too_short, StatusCode::BAD_REQUEST);

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/loa/me",
            Some(&submitter.session_token),
            Some(json!({
                "start": start.to_rfc3339(),
                "end": end.to_rfc3339(),
                "reason": "Vacation"
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let loa_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["status"], "PENDING");

    // Self-listing shows it.
    let self_list = app
        .json_request(
            "GET",
            "/api/v1/loa/me",
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&self_list, StatusCode::OK);
    let self_list_body: Value = json_body(self_list).await;
    assert_eq!(self_list_body["total"], 1);

    // Admin approves it.
    let approve_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/loa/{loa_id}/decision"),
            Some(&staff.session_token),
            Some(json!({"status": "APPROVED"})),
        )
        .await;
    assert_status(&approve_response, StatusCode::OK);
    let approved: Value = json_body(approve_response).await;
    assert_eq!(approved["status"], "APPROVED");

    // Self-update still works on an APPROVED LOA and resets it to PENDING
    // (this is the parity fix: editing an approved LOA "cancels" the approval).
    let new_start = start + Duration::days(1);
    let new_end = new_start + Duration::days(10);
    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/loa/{loa_id}"),
            Some(&submitter.session_token),
            Some(json!({
                "start": new_start.to_rfc3339(),
                "end": new_end.to_rfc3339(),
                "reason": "Vacation, updated dates"
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let updated: Value = json_body(update_response).await;
    assert_eq!(updated["status"], "PENDING");
    assert!(updated["decided_at"].is_null());

    // A different user cannot update or cancel someone else's LOA.
    let denied_update = app
        .json_request(
            "PATCH",
            &format!("/api/v1/loa/{loa_id}"),
            Some(&other_user.session_token),
            Some(json!({
                "start": new_start.to_rfc3339(),
                "end": new_end.to_rfc3339(),
                "reason": "Not mine"
            })),
        )
        .await;
    assert_status(&denied_update, StatusCode::NOT_FOUND);

    let denied_cancel = app
        .json_request(
            "POST",
            &format!("/api/v1/loa/{loa_id}/cancel"),
            Some(&other_user.session_token),
            None,
        )
        .await;
    assert_status(&denied_cancel, StatusCode::NOT_FOUND);

    // Admin approves again, then the submitter self-cancels — this is the
    // capability that didn't exist before this pass (self-service cancel
    // regardless of status).
    let reapprove_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/loa/{loa_id}/decision"),
            Some(&staff.session_token),
            Some(json!({"status": "APPROVED"})),
        )
        .await;
    assert_status(&reapprove_response, StatusCode::OK);

    let cancel_response = app
        .json_request(
            "POST",
            &format!("/api/v1/loa/{loa_id}/cancel"),
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&cancel_response, StatusCode::OK);
    let cancelled: Value = json_body(cancel_response).await;
    assert_eq!(cancelled["status"], "INACTIVE");
    // Self-cancel is not a staff decision — decided_by_actor_id from the
    // earlier admin approval is left untouched, not cleared or overwritten
    // with the submitter's own actor id.
    assert!(cancelled["decided_by_actor_id"].is_string());

    // Once INACTIVE, both self-update and self-cancel are rejected.
    let update_after_cancel = app
        .json_request(
            "PATCH",
            &format!("/api/v1/loa/{loa_id}"),
            Some(&submitter.session_token),
            Some(json!({
                "start": new_start.to_rfc3339(),
                "end": new_end.to_rfc3339(),
                "reason": "Too late"
            })),
        )
        .await;
    assert_status(&update_after_cancel, StatusCode::NOT_FOUND);

    let cancel_again = app
        .json_request(
            "POST",
            &format!("/api/v1/loa/{loa_id}/cancel"),
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&cancel_again, StatusCode::NOT_FOUND);

    // Admin listing includes the denormalized cid/display_name and supports
    // the status filter.
    let admin_list = app
        .json_request(
            "GET",
            "/api/v1/admin/loa?status=INACTIVE",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&admin_list, StatusCode::OK);
    let admin_list_body: Value = json_body(admin_list).await;
    let items = admin_list_body["items"].as_array().unwrap();
    let item = items
        .iter()
        .find(|i| i["id"] == loa_id)
        .expect("cancelled loa present in admin INACTIVE filter");
    assert_eq!(item["cid"], 10000097);
    assert_eq!(item["display_name"], "Loa Submitter");

    // display_name contains-filter matches and excludes correctly.
    let matching_filter = app
        .json_request(
            "GET",
            "/api/v1/admin/loa?display_name=Submitter",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&matching_filter, StatusCode::OK);
    let matching: Value = json_body(matching_filter).await;
    assert_eq!(matching["total"], 1);

    let non_matching_filter = app
        .json_request(
            "GET",
            "/api/v1/admin/loa?display_name=NoSuchController",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&non_matching_filter, StatusCode::OK);
    let non_matching: Value = json_body(non_matching_filter).await;
    assert_eq!(non_matching["total"], 0);
}
