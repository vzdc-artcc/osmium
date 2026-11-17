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

// Email enabled so the decision email lands in `email.outbox` and its absence on a refused
// decision means something. VATUSA points at a closed local port with a dummy key, so an
// approval that got as far as `sync_approved_visitor_to_vatusa` fails with 503 here instead
// of reaching the real API.
const DECIDED_APPLICATION_ENV: &[(&str, &str)] = &[
    ("EMAIL_ENABLED", "true"),
    ("AWS_REGION", "us-east-1"),
    ("AWS_ACCESS_KEY_ID", "test-key"),
    ("AWS_SECRET_ACCESS_KEY", "test-secret"),
    ("EMAIL_FROM_ADDRESS", "noreply@example.invalid"),
    ("VATUSA_API_KEY", "test-vatusa-key"),
    ("VATUSA_API_BASE_URL", "http://127.0.0.1:1"),
];

async fn visitor_application_side_effect_counts(
    app: &TestApp,
    application_id: &str,
    user_id: &str,
) -> (i64, i64) {
    let audit_rows: i64 = sqlx::query_scalar(
        "select count(*) from access.audit_logs \
         where resource_type = 'VISITOR_APPLICATION' and action = 'UPDATE' and resource_id = $1",
    )
    .bind(application_id)
    .fetch_one(&app.pool)
    .await
    .expect("count audit rows");
    let outbox_rows: i64 = sqlx::query_scalar(
        "select count(*) from email.outbox o \
         join email.outbox_recipients r on r.outbox_id = o.id \
         where o.template_id in ('visitor.accepted', 'visitor.rejected') and r.user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .expect("count outbox rows");
    (audit_rows, outbox_rows)
}

async fn visitor_application_decision(
    app: &TestApp,
    application_id: &str,
) -> (
    String,
    Option<String>,
    Option<chrono::DateTime<chrono::Utc>>,
) {
    sqlx::query_as(
        "select status, reason_for_denial, decided_at \
         from org.visitor_applications where id = $1",
    )
    .bind(application_id)
    .fetch_one(&app.pool)
    .await
    .expect("visitor application row")
}

async fn membership_state(app: &TestApp, user_id: &str) -> Option<(String, String, bool)> {
    sqlx::query_as(
        "select controller_status, membership_status, is_active \
         from org.memberships where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&app.pool)
    .await
    .expect("membership lookup")
}

#[tokio::test(flavor = "current_thread")]
async fn deciding_a_decided_visitor_application_is_refused_and_changes_nothing() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new_with_env_overrides(DECIDED_APPLICATION_ENV).await else {
        return;
    };
    app.state
        .email
        .sync_template_registry(&app.pool)
        .await
        .expect("sync template registry");
    assert!(
        app.state.email.is_available(),
        "email transport must be available for this test"
    );

    let staff = app
        .create_user(
            10017301,
            "Decided Visitor Staff",
            &["users.visitor_applications.decide"],
        )
        .await;
    let self_perms = &["users.visitor_applications_self.request"];
    let denied_applicant = app
        .create_user(10017302, "Denied Applicant", self_perms)
        .await;
    let approved_applicant = app
        .create_user(10017303, "Approved Applicant", self_perms)
        .await;

    let mut application_ids = Vec::new();
    for applicant in [&denied_applicant, &approved_applicant] {
        let response = app
            .json_request(
                "POST",
                "/api/v1/users/visitor-application",
                Some(&applicant.session_token),
                Some(json!({"home_facility": "ZLA", "why_visit": "Visiting."})),
            )
            .await;
        assert_status(&response, StatusCode::OK);
        let body: Value = json_body(response).await;
        application_ids.push(body["id"].as_str().unwrap().to_string());
    }
    let (denied_id, approved_id) = (application_ids[0].clone(), application_ids[1].clone());
    let decide_path = |id: &str| format!("/api/v1/admin/visitor-applications/{id}");

    // Positive control: the first decision writes one audit row and queues one email.
    let first_deny = app
        .json_request(
            "PATCH",
            &decide_path(&denied_id),
            Some(&staff.session_token),
            Some(json!({"status": "DENIED", "reason_for_denial": "First reason."})),
        )
        .await;
    assert_status(&first_deny, StatusCode::OK);
    assert_eq!(
        visitor_application_side_effect_counts(&app, &denied_id, &denied_applicant.id).await,
        (1, 1),
        "the first decision writes one audit row and one outbox row"
    );
    let denied_before = visitor_application_decision(&app, &denied_id).await;
    assert_eq!(denied_before.0, "DENIED");
    let denied_membership_before = membership_state(&app, &denied_applicant.id).await;
    assert_ne!(
        denied_membership_before.as_ref().map(|m| m.0.as_str()),
        Some("VISITOR"),
        "a denied applicant is not a visitor"
    );

    // Approving a denied application is refused before the VATUSA call: reaching it would
    // answer 503 from the closed port.
    let approve_denied = app
        .json_request(
            "PATCH",
            &decide_path(&denied_id),
            Some(&staff.session_token),
            Some(json!({"status": "APPROVED"})),
        )
        .await;
    assert_status(&approve_denied, StatusCode::BAD_REQUEST);

    let deny_again = app
        .json_request(
            "PATCH",
            &decide_path(&denied_id),
            Some(&staff.session_token),
            Some(json!({"status": "DENIED", "reason_for_denial": "Second reason."})),
        )
        .await;
    assert_status(&deny_again, StatusCode::BAD_REQUEST);

    assert_eq!(
        visitor_application_decision(&app, &denied_id).await,
        denied_before,
        "a refused decision leaves the stored decision as it was"
    );
    assert_eq!(
        visitor_application_side_effect_counts(&app, &denied_id, &denied_applicant.id).await,
        (1, 1),
        "a refused decision writes no audit row and queues no email"
    );
    assert_eq!(
        membership_state(&app, &denied_applicant.id).await,
        denied_membership_before,
        "approving a denied application leaves the membership as it was"
    );

    // Approve the second application through the repo (the HTTP approval calls VATUSA), then
    // take the controller off the roster as staff would after the approval.
    let mut tx = app.pool.begin().await.expect("begin tx");
    osmium::repos::users::decide_visitor_application(
        &mut tx,
        &approved_id,
        "APPROVED",
        None,
        None,
        "ZDC",
    )
    .await
    .expect("decide_visitor_application succeeds")
    .expect("pending application is decided");
    tx.commit().await.expect("commit tx");
    sqlx::query(
        "update org.memberships set membership_status = 'INACTIVE', is_active = false \
         where user_id = $1",
    )
    .bind(&approved_applicant.id)
    .execute(&app.pool)
    .await
    .expect("deactivate membership");
    let approved_before = visitor_application_decision(&app, &approved_id).await;
    assert_eq!(approved_before.0, "APPROVED");
    let inactive = Some(("VISITOR".to_string(), "INACTIVE".to_string(), false));
    assert_eq!(
        membership_state(&app, &approved_applicant.id).await,
        inactive
    );

    for payload in [
        json!({"status": "APPROVED"}),
        json!({"status": "DENIED", "reason_for_denial": "Changed my mind."}),
    ] {
        let response = app
            .json_request(
                "PATCH",
                &decide_path(&approved_id),
                Some(&staff.session_token),
                Some(payload.clone()),
            )
            .await;
        assert_status(&response, StatusCode::BAD_REQUEST);
        assert_eq!(
            visitor_application_decision(&app, &approved_id).await,
            approved_before,
            "{payload} on an approved application leaves its decision as it was"
        );
        assert_eq!(
            membership_state(&app, &approved_applicant.id).await,
            inactive,
            "{payload} on an approved application leaves the membership inactive"
        );
    }
    assert_eq!(
        visitor_application_side_effect_counts(&app, &approved_id, &approved_applicant.id).await,
        (0, 0),
        "refused decisions on an approved application write no audit row and queue no email"
    );

    // The repo refuses on its own, for a decide that passed the handler's status check
    // before a concurrent one committed.
    let mut tx = app.pool.begin().await.expect("begin tx");
    let redecided = osmium::repos::users::decide_visitor_application(
        &mut tx,
        &approved_id,
        "APPROVED",
        None,
        None,
        "ZDC",
    )
    .await
    .expect("decide_visitor_application succeeds");
    tx.commit().await.expect("commit tx");
    assert!(
        redecided.is_none(),
        "an approved application is not decided again"
    );
    assert_eq!(
        membership_state(&app, &approved_applicant.id).await,
        inactive,
        "the repo leaves the membership inactive when nothing was decided"
    );

    app.cleanup().await;
}
