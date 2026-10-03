mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn visitor_application_lifecycle_works_end_to_end() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let applicant = app
        .create_user(
            10000100,
            "Visitor Applicant",
            &[
                "users.visitor_applications_self.read",
                "users.visitor_applications_self.request",
            ],
        )
        .await;
    let staff = app
        .create_user(
            10000101,
            "Visitor Staff",
            &[
                "users.visitor_applications.read",
                "users.visitor_applications.decide",
            ],
        )
        .await;
    let unauthorized = app.create_user(10000102, "Visitor No Perms", &[]).await;

    // No application yet.
    let initial = app
        .json_request(
            "GET",
            "/api/v1/users/visitor-application",
            Some(&applicant.session_token),
            None,
        )
        .await;
    assert_status(&initial, StatusCode::OK);
    let initial_body: Value = json_body(initial).await;
    assert!(initial_body.is_null());

    // A user without self permission is rejected.
    let denied_create = app
        .json_request(
            "POST",
            "/api/v1/users/visitor-application",
            Some(&unauthorized.session_token),
            Some(json!({"home_facility": "ZLA", "why_visit": "Testing"})),
        )
        .await;
    assert_status(&denied_create, StatusCode::UNAUTHORIZED);

    // Create the application.
    let create_response = app
        .json_request(
            "POST",
            "/api/v1/users/visitor-application",
            Some(&applicant.session_token),
            Some(json!({"home_facility": "zla", "why_visit": "I want to control DCA."})),
        )
        .await;
    assert_status(&create_response, StatusCode::OK);
    let created: Value = json_body(create_response).await;
    let application_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["status"], "PENDING");
    assert_eq!(created["home_facility"], "ZLA");
    assert_eq!(created["cid"], 10000100);

    // Self GET now returns it.
    let self_get = app
        .json_request(
            "GET",
            "/api/v1/users/visitor-application",
            Some(&applicant.session_token),
            None,
        )
        .await;
    assert_status(&self_get, StatusCode::OK);
    let self_get_body: Value = json_body(self_get).await;
    assert_eq!(self_get_body["id"], application_id);

    // Admin list includes it (this exercises the admin count query directly —
    // previously broken by a wrong schema name, `training.visitor_applications`
    // instead of `org.visitor_applications`, which would 500 here).
    let admin_list = app
        .json_request(
            "GET",
            "/api/v1/admin/visitor-applications",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&admin_list, StatusCode::OK);
    let admin_list_body: Value = json_body(admin_list).await;
    assert_eq!(admin_list_body["total"], 1);
    let items = admin_list_body["items"].as_array().unwrap();
    let item = items
        .iter()
        .find(|i| i["id"] == application_id)
        .expect("application present in admin list");
    assert_eq!(item["display_name"], "Visitor Applicant");

    // display_name and home_facility contains-filters match and exclude correctly.
    let matching = app
        .json_request(
            "GET",
            "/api/v1/admin/visitor-applications?display_name=Applicant&home_facility=ZL",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&matching, StatusCode::OK);
    let matching_body: Value = json_body(matching).await;
    assert_eq!(matching_body["total"], 1);

    let non_matching = app
        .json_request(
            "GET",
            "/api/v1/admin/visitor-applications?home_facility=ZDV",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&non_matching, StatusCode::OK);
    let non_matching_body: Value = json_body(non_matching).await;
    assert_eq!(non_matching_body["total"], 0);

    // Denying without a reason is rejected.
    let deny_no_reason = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/visitor-applications/{application_id}"),
            Some(&staff.session_token),
            Some(json!({"status": "DENIED"})),
        )
        .await;
    assert_status(&deny_no_reason, StatusCode::BAD_REQUEST);

    // Deny with a reason.
    let deny_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/visitor-applications/{application_id}"),
            Some(&staff.session_token),
            Some(json!({"status": "DENIED", "reason_for_denial": "Not in good standing."})),
        )
        .await;
    assert_status(&deny_response, StatusCode::OK);
    let denied: Value = json_body(deny_response).await;
    assert_eq!(denied["status"], "DENIED");
    assert_eq!(denied["reason_for_denial"], "Not in good standing.");

    // Resubmitting (upsert) resets it to PENDING and clears the denial reason.
    let resubmit_response = app
        .json_request(
            "POST",
            "/api/v1/users/visitor-application",
            Some(&applicant.session_token),
            Some(json!({"home_facility": "ZDC", "why_visit": "Trying again."})),
        )
        .await;
    assert_status(&resubmit_response, StatusCode::OK);
    let resubmitted: Value = json_body(resubmit_response).await;
    assert_eq!(resubmitted["id"], application_id);
    assert_eq!(resubmitted["status"], "PENDING");
    assert!(resubmitted["reason_for_denial"].is_null());

    // Approving via the real HTTP handler also calls the live VATUSA
    // `manageVisitor` API (`sync_approved_visitor_to_vatusa`) — a genuine
    // external network call with no test-mode bypass. Exercising that
    // through this suite would either depend on real VATUSA state (flaky,
    // slow) or, worse, actually mutate a real facility roster for a fake
    // test cid — which is exactly what happened once already before this
    // was caught. So the HTTP-level APPROVED path is deliberately NOT
    // exercised here; instead this calls the repo layer directly to verify
    // the membership-activation side effect (the part that's actually this
    // codebase's responsibility) without going anywhere near the network.
    let mut tx = app.pool.begin().await.expect("begin tx");
    let approved = osmium::repos::users::decide_visitor_application(
        &mut tx,
        &application_id,
        "APPROVED",
        None,
        None,
        "ZDC",
    )
    .await
    .expect("decide_visitor_application succeeds")
    .expect("application exists");
    tx.commit().await.expect("commit tx");
    assert_eq!(approved.status, "APPROVED");

    let membership = sqlx::query_as::<_, (String, String)>(
        "select controller_status, membership_status from org.memberships where user_id = $1",
    )
    .bind(&applicant.id)
    .fetch_one(&app.pool)
    .await
    .expect("membership row exists after approval");
    assert_eq!(membership.0, "VISITOR");
    assert_eq!(membership.1, "ACTIVE");
}
