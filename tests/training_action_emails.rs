//! OTS recommendation, training assignment and trainer release actions queue their
//! notifications in `email.outbox`, to the student and trainers each change
//! affects, in the same transaction as the change. DB-backed; skips without
//! `DATABASE_URL`.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, TestUser, assert_status, json_body, lock_env};

const EMAIL_ENV: &[(&str, &str)] = &[
    ("EMAIL_ENABLED", "true"),
    ("AWS_REGION", "us-east-1"),
    ("AWS_ACCESS_KEY_ID", "test-key"),
    ("AWS_SECRET_ACCESS_KEY", "test-secret"),
    ("EMAIL_FROM_ADDRESS", "noreply@example.invalid"),
];

type Queued = BTreeMap<String, BTreeSet<String>>;

async fn email_app() -> Option<TestApp> {
    let app = TestApp::new_with_env_overrides(EMAIL_ENV).await?;
    app.state
        .email
        .sync_template_registry(&app.pool)
        .await
        .expect("sync template registry");
    assert!(app.state.email.is_available(), "email must be enabled");
    Some(app)
}

/// Every queued template with its recipient user ids, then empties the outbox so
/// the next step starts from nothing.
async fn drain(app: &TestApp) -> Queued {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        select o.template_id, r.user_id
        from email.outbox o
        join email.outbox_recipients r on r.outbox_id = o.id
        "#,
    )
    .fetch_all(&app.pool)
    .await
    .expect("read outbox");
    sqlx::query("delete from email.outbox")
        .execute(&app.pool)
        .await
        .expect("clear outbox");

    let mut queued = Queued::new();
    for (template, user_id) in rows {
        queued.entry(template).or_default().insert(user_id);
    }
    queued
}

/// The `release_request_approved` flag on the queued `training.assignment_released`.
async fn released_flag(app: &TestApp) -> Value {
    sqlx::query_scalar(
        "select payload->'release_request_approved' from email.outbox where template_id = 'training.assignment_released'",
    )
    .fetch_one(&app.pool)
    .await
    .expect("read released payload")
}

fn expect(entries: &[(&str, &[&TestUser])]) -> Queued {
    entries
        .iter()
        .map(|(template, users)| {
            (
                template.to_string(),
                users.iter().map(|user| user.id.clone()).collect(),
            )
        })
        .collect()
}

async fn send(app: &TestApp, method: &str, uri: &str, actor: &TestUser, body: Value) -> Value {
    let body = (method != "DELETE").then_some(body);
    let response = app
        .json_request(method, uri, Some(&actor.session_token), body)
        .await;
    let status = response.status();
    assert!(status.is_success(), "{method} {uri} returned {status}");
    if status == StatusCode::NO_CONTENT {
        Value::Null
    } else {
        json_body(response).await
    }
}

#[tokio::test]
async fn assignment_create_update_delete_notify_student_and_trainers() {
    let _env = lock_env();
    let Some(app) = email_app().await else {
        return;
    };
    let staff = app
        .create_user(
            1_260_001,
            "Assignment Staff",
            &[
                "training.assignments.create",
                "training.assignments.update",
                "training.assignments.delete",
            ],
        )
        .await;
    let student = app.create_user(1_260_002, "Student", &[]).await;
    let primary = app.create_user(1_260_003, "Primary", &[]).await;
    let trainer_a = app.create_user(1_260_004, "Trainer A", &[]).await;
    let trainer_b = app.create_user(1_260_005, "Trainer B", &[]).await;
    let trainer_c = app.create_user(1_260_006, "Trainer C", &[]).await;

    let created = send(
        &app,
        "POST",
        "/api/v1/training/assignments",
        &staff,
        json!({
            "student_id": student.id,
            "primary_trainer_id": primary.id,
            "other_trainer_ids": [trainer_a.id, trainer_b.id]
        }),
    )
    .await;
    let uri = format!(
        "/api/v1/training/assignments/{}",
        created["id"].as_str().unwrap()
    );
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.assignment_created", &[&student]),
            ("training.assignment_primary_trainer", &[&primary]),
            (
                "training.assignment_other_trainer",
                &[&trainer_a, &trainer_b]
            ),
        ])
    );
    let staff_actor: String = sqlx::query_scalar("select id from access.actors where user_id = $1")
        .bind(&staff.id)
        .fetch_one(&app.pool)
        .await
        .expect("staff actor");
    let queue_audit_actors: Vec<Option<String>> = sqlx::query_scalar(
        "select actor_id from access.audit_logs where action = 'QUEUE' and resource_type = 'EMAIL'",
    )
    .fetch_all(&app.pool)
    .await
    .expect("read queue audits");
    assert_eq!(
        queue_audit_actors,
        vec![Some(staff_actor); 3],
        "each queued email writes a QUEUE audit row attributed to the staff actor"
    );

    // Trainer A becomes primary, the old primary drops out, C joins.
    let update = json!({
        "primary_trainer_id": trainer_a.id,
        "other_trainer_ids": [trainer_b.id, trainer_c.id]
    });
    send(&app, "PATCH", &uri, &staff, update.clone()).await;
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.assignment_trainer_removed", &[&primary]),
            ("training.assignment_other_trainer", &[&trainer_c]),
            ("training.assignment_primary_trainer", &[&trainer_a]),
            ("training.assignment_updated", &[&student]),
        ])
    );

    // The same trainer set again changes nothing and sends nothing.
    send(&app, "PATCH", &uri, &staff, update).await;
    assert_eq!(
        drain(&app).await,
        Queued::new(),
        "no-op update sends nothing"
    );

    send(&app, "DELETE", &uri, &staff, Value::Null).await;
    assert_eq!(released_flag(&app).await, json!(false));
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.assignment_released", &[&student]),
            (
                "training.assignment_trainer_removed",
                &[&trainer_a, &trainer_b, &trainer_c]
            ),
        ])
    );

    app.cleanup().await;
}

#[tokio::test]
async fn ots_instructor_changes_and_delete_notify_student_and_instructors() {
    let _env = lock_env();
    let Some(app) = email_app().await else {
        return;
    };
    let staff = app
        .create_user(
            1_260_011,
            "OTS Staff",
            &[
                "training.ots_recommendations.create",
                "training.ots_recommendations.update",
                "training.ots_recommendations.delete",
            ],
        )
        .await;
    let student = app.create_user(1_260_012, "OTS Student", &[]).await;
    let first = app.create_user(1_260_013, "First Instructor", &[]).await;
    let second = app.create_user(1_260_014, "Second Instructor", &[]).await;

    let created = send(
        &app,
        "POST",
        "/api/v1/training/ots-recommendations",
        &staff,
        json!({ "student_id": student.id, "notes": "Ready for OTS" }),
    )
    .await;
    let uri = format!(
        "/api/v1/training/ots-recommendations/{}",
        created["id"].as_str().unwrap()
    );
    drain(&app).await;

    send(
        &app,
        "PATCH",
        &uri,
        &staff,
        json!({ "assigned_instructor_id": first.id }),
    )
    .await;
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.ots_assigned_student", &[&student]),
            ("training.ots_assigned_instructor", &[&first]),
        ])
    );

    let reassign = json!({ "assigned_instructor_id": second.id });
    send(&app, "PATCH", &uri, &staff, reassign.clone()).await;
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.ots_unassigned_instructor", &[&first]),
            ("training.ots_assigned_student", &[&student]),
            ("training.ots_assigned_instructor", &[&second]),
        ])
    );

    send(&app, "PATCH", &uri, &staff, reassign).await;
    assert_eq!(
        drain(&app).await,
        Queued::new(),
        "same instructor sends nothing"
    );

    send(
        &app,
        "PATCH",
        &uri,
        &staff,
        json!({ "assigned_instructor_id": null }),
    )
    .await;
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.ots_unassigned_student", &[&student]),
            ("training.ots_unassigned_instructor", &[&second]),
        ])
    );

    send(
        &app,
        "PATCH",
        &uri,
        &staff,
        json!({ "assigned_instructor_id": first.id }),
    )
    .await;
    drain(&app).await;
    send(&app, "DELETE", &uri, &staff, Value::Null).await;
    assert_eq!(
        drain(&app).await,
        expect(&[("training.ots_deleted", &[&student, &first])])
    );

    app.cleanup().await;
}

#[tokio::test]
async fn release_approval_notifies_once_and_denial_sends_nothing() {
    let _env = lock_env();
    let Some(app) = email_app().await else {
        return;
    };
    let staff = app
        .create_user(
            1_260_021,
            "Release Staff",
            &[
                "training.assignments.create",
                "training.release_requests.decide",
            ],
        )
        .await;
    let student = app
        .create_user(
            1_260_022,
            "Release Student",
            &["training.release_requests_self.request"],
        )
        .await;
    let other_student = app
        .create_user(
            1_260_023,
            "Denied Student",
            &["training.release_requests_self.request"],
        )
        .await;
    let trainer = app.create_user(1_260_024, "Release Trainer", &[]).await;

    send(
        &app,
        "POST",
        "/api/v1/training/assignments",
        &staff,
        json!({ "student_id": student.id, "primary_trainer_id": trainer.id }),
    )
    .await;
    let approved = send(
        &app,
        "POST",
        "/api/v1/training/trainer-release-requests",
        &student,
        json!({}),
    )
    .await;
    let denied = send(
        &app,
        "POST",
        "/api/v1/training/trainer-release-requests",
        &other_student,
        json!({}),
    )
    .await;
    drain(&app).await;

    let approve_uri = format!(
        "/api/v1/training/trainer-release-requests/{}",
        approved["id"].as_str().unwrap()
    );
    send(
        &app,
        "PATCH",
        &approve_uri,
        &staff,
        json!({ "status": "APPROVED" }),
    )
    .await;
    assert_eq!(released_flag(&app).await, json!(true));
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.assignment_released", &[&student]),
            ("training.assignment_trainer_removed", &[&trainer]),
        ])
    );

    // Re-approving after a new assignment exists ends that assignment too, so its
    // trainer and the student are told.
    let new_trainer = app.create_user(1_260_025, "New Trainer", &[]).await;
    send(
        &app,
        "POST",
        "/api/v1/training/assignments",
        &staff,
        json!({ "student_id": student.id, "primary_trainer_id": new_trainer.id }),
    )
    .await;
    drain(&app).await;
    send(
        &app,
        "PATCH",
        &approve_uri,
        &staff,
        json!({ "status": "APPROVED" }),
    )
    .await;
    assert_eq!(
        drain(&app).await,
        expect(&[
            ("training.assignment_released", &[&student]),
            ("training.assignment_trainer_removed", &[&new_trainer]),
        ])
    );

    send(
        &app,
        "PATCH",
        &approve_uri,
        &staff,
        json!({ "status": "APPROVED" }),
    )
    .await;
    assert_eq!(
        drain(&app).await,
        Queued::new(),
        "re-approval sends nothing"
    );

    send(
        &app,
        "PATCH",
        &format!(
            "/api/v1/training/trainer-release-requests/{}",
            denied["id"].as_str().unwrap()
        ),
        &staff,
        json!({ "status": "DENIED" }),
    )
    .await;
    assert_eq!(drain(&app).await, Queued::new(), "denial sends nothing");

    app.cleanup().await;
}

#[tokio::test]
async fn failed_enqueue_rolls_back_the_action_and_missing_address_is_skipped() {
    let _env = lock_env();
    let Some(app) = email_app().await else {
        return;
    };
    let staff = app
        .create_user(
            1_260_031,
            "Rollback Staff",
            &["training.assignments.create"],
        )
        .await;
    let student = app.create_user(1_260_032, "Rollback Student", &[]).await;
    let trainer = app.create_user(1_260_033, "Rollback Trainer", &[]).await;
    let body = json!({ "student_id": student.id, "primary_trainer_id": trainer.id });

    // A student with no address is skipped; the trainer is still notified.
    sqlx::query("update identity.users set email = null where id = $1")
        .bind(&student.id)
        .execute(&app.pool)
        .await
        .expect("clear student email");
    let created = send(
        &app,
        "POST",
        "/api/v1/training/assignments",
        &staff,
        body.clone(),
    )
    .await;
    assert_eq!(
        drain(&app).await,
        expect(&[("training.assignment_primary_trainer", &[&trainer])])
    );
    sqlx::query("delete from training.training_assignments where id = $1")
        .bind(created["id"].as_str().unwrap())
        .execute(&app.pool)
        .await
        .expect("remove assignment");

    // Without branding the email cannot render, so the whole action rolls back.
    sqlx::query("delete from email.branding")
        .execute(&app.pool)
        .await
        .expect("remove branding");
    let response = app
        .json_request(
            "POST",
            "/api/v1/training/assignments",
            Some(&staff.session_token),
            Some(body),
        )
        .await;
    assert_status(&response, StatusCode::INTERNAL_SERVER_ERROR);
    let assignments: i64 = sqlx::query_scalar("select count(*) from training.training_assignments")
        .fetch_one(&app.pool)
        .await
        .expect("count assignments");
    assert_eq!(assignments, 0, "the assignment rolled back with its email");
    assert_eq!(drain(&app).await, Queued::new());

    app.cleanup().await;
}
