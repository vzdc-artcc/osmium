//! Proves that a domain event actually lands a row in `email.outbox` — the
//! end-to-end chain handler -> `enqueue_to_users` -> render-preview -> outbox
//! insert, including the template-registry sync that satisfies the outbox FK.
//!
//! Uses feedback-release as the representative path: it's the least-gated of the
//! newly-wired notifications (its template has `respect_user_event_pref = false`,
//! so a seeded controller with an email is always a deliverable recipient). Every
//! other wired notification funnels through the exact same `enqueue_to_users`
//! path, so this guards the mechanism as a whole.
//!
//! Requires `DATABASE_URL` (CI provides Postgres); skips cleanly otherwise.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

// Fake AWS creds + from-address flip the email service to `is_available()` so
// enqueues persist to the outbox; SES itself is never contacted (only the
// delivery worker sends, and tests don't run it).
const EMAIL_ENV: &[(&str, &str)] = &[
    ("EMAIL_ENABLED", "true"),
    ("AWS_REGION", "us-east-1"),
    ("AWS_ACCESS_KEY_ID", "test-key"),
    ("AWS_SECRET_ACCESS_KEY", "test-secret"),
    ("EMAIL_FROM_ADDRESS", "noreply@example.invalid"),
];

#[tokio::test]
async fn feedback_release_enqueues_outbox_email() {
    let _env = lock_env();
    let Some(app) = TestApp::new_with_env_overrides(EMAIL_ENV).await else {
        return;
    };

    // Mirror the startup registry sync so `feedback.new` exists in email.templates
    // and satisfies the outbox foreign key (migration 0031 only seeds a subset).
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
            1000,
            "Staff Member",
            &["feedback.items.create", "feedback.items.decide"],
        )
        .await;
    let target = app.create_user(2000, "Target Controller", &[]).await;

    // Submit feedback about the target controller.
    let create = app
        .json_request(
            "POST",
            "/api/v1/feedback",
            Some(&staff.session_token),
            Some(json!({
                "target_cid": target.cid,
                "pilot_callsign": "AAL123",
                "controller_position": "DCA_TWR",
                "rating": 5,
                "comments": "Great service"
            })),
        )
        .await;
    assert_status(&create, StatusCode::CREATED);
    let created: Value = json_body(create).await;
    let feedback_id = created["id"].as_str().expect("feedback id").to_string();

    // Nothing queued while it is still PENDING.
    assert_eq!(
        outbox_count(&app, "feedback.new").await,
        0,
        "no feedback email before release"
    );

    // Release it — this is the event that should enqueue `feedback.new`.
    let release = app
        .json_request(
            "PATCH",
            &format!("/api/v1/feedback/{feedback_id}"),
            Some(&staff.session_token),
            Some(json!({ "status": "RELEASED" })),
        )
        .await;
    assert_status(&release, StatusCode::OK);

    // The email is now in the outbox, addressed to the target controller.
    assert_eq!(
        outbox_count(&app, "feedback.new").await,
        1,
        "feedback.new queued on release"
    );
    let recipient_rows: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from email.outbox_recipients r
        join email.outbox o on o.id = r.outbox_id
        where o.template_id = 'feedback.new' and r.user_id = $1
        "#,
    )
    .bind(&target.id)
    .fetch_one(&app.pool)
    .await
    .expect("query outbox recipients");
    assert_eq!(recipient_rows, 1, "target controller is the recipient");

    // Re-releasing an already-released item must not enqueue a duplicate.
    let re_release = app
        .json_request(
            "PATCH",
            &format!("/api/v1/feedback/{feedback_id}"),
            Some(&staff.session_token),
            Some(json!({ "status": "RELEASED" })),
        )
        .await;
    assert_status(&re_release, StatusCode::OK);
    assert_eq!(
        outbox_count(&app, "feedback.new").await,
        1,
        "no duplicate email on re-release"
    );

    app.cleanup().await;
}

async fn outbox_count(app: &TestApp, template_id: &str) -> i64 {
    sqlx::query_scalar("select count(*) from email.outbox where template_id = $1")
        .bind(template_id)
        .fetch_one(&app.pool)
        .await
        .expect("query outbox count")
}
