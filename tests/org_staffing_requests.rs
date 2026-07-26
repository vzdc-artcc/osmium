mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn staffing_request_crud_lifecycle_and_admin_filters_work_end_to_end() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000090, "Staffing Submitter", &["auth.profile.read"])
        .await;
    let staff = app
        .create_user(
            10000091,
            "Staffing Staff",
            &["org.staffing_requests.read", "org.staffing_requests.delete"],
        )
        .await;
    let event_staff = app
        .create_user(
            10000092,
            "Staffing Event Staff",
            &["org.staffing_requests.read", "org.staffing_requests.delete"],
        )
        .await;
    let unauthorized = app.create_user(10000093, "Staffing No Perms", &[]).await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/staffing-requests/me",
            Some(&submitter.session_token),
            Some(json!({
                "name": "SEP 2026 Fly-In",
                "description": "Need coverage for tower and approach all weekend."
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let request_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "SEP 2026 Fly-In");

    // Self-service listing returns only the submitter's own requests.
    let self_list_response = app
        .json_request(
            "GET",
            "/api/v1/staffing-requests/me",
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&self_list_response, StatusCode::OK);
    let self_list: Value = json_body(self_list_response).await;
    assert_eq!(self_list["total"], 1);

    // A user with neither permission cannot list or delete.
    let denied_list = app
        .json_request(
            "GET",
            "/api/v1/admin/staffing-requests",
            Some(&unauthorized.session_token),
            None,
        )
        .await;
    assert_status(&denied_list, StatusCode::UNAUTHORIZED);

    // Both STAFF and EVENT_STAFF can list, and denormalized fields (including the
    // new email field) are populated.
    for admin in [&staff, &event_staff] {
        let list_response = app
            .json_request(
                "GET",
                "/api/v1/admin/staffing-requests",
                Some(&admin.session_token),
                None,
            )
            .await;
        assert_status(&list_response, StatusCode::OK);
        let list: Value = json_body(list_response).await;
        let items = list["items"].as_array().unwrap();
        let item = items
            .iter()
            .find(|i| i["id"] == request_id)
            .expect("created request present in admin list");
        assert_eq!(item["cid"], 10000090);
        assert_eq!(item["display_name"], "Staffing Submitter");
        assert!(item["email"].is_string());
    }

    // display_name contains-filter matches and excludes correctly.
    let matching_filter = app
        .json_request(
            "GET",
            "/api/v1/admin/staffing-requests?display_name=Submitter",
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
            "/api/v1/admin/staffing-requests?display_name=NoSuchController",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&non_matching_filter, StatusCode::OK);
    let non_matching: Value = json_body(non_matching_filter).await;
    assert_eq!(non_matching["total"], 0);

    // Deleting requires the delete permission; the unauthorized user is rejected.
    let denied_delete = app
        .json_request(
            "DELETE",
            &format!("/api/v1/admin/staffing-requests/{request_id}"),
            Some(&unauthorized.session_token),
            None,
        )
        .await;
    assert_status(&denied_delete, StatusCode::UNAUTHORIZED);

    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/admin/staffing-requests/{request_id}"),
            Some(&event_staff.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::OK);

    let after_delete = app
        .json_request(
            "GET",
            "/api/v1/staffing-requests/me",
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&after_delete, StatusCode::OK);
    let after_delete_body: Value = json_body(after_delete).await;
    assert_eq!(after_delete_body["total"], 0);
}
