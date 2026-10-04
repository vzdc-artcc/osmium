mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, TestUser, assert_status, json_body, lock_env};

async fn get_list(app: &TestApp, user: &TestUser, uri: &str) -> Value {
    let response = app
        .json_request("GET", uri, Some(&user.session_token), None)
        .await;
    assert_status(&response, StatusCode::OK);
    json_body(response).await
}

/// Asserts both the page contents and `total` agree with `expected`, so a filter applied to only
/// the list query or only the count query fails here.
fn assert_items_and_total(body: &Value, expected: usize) {
    assert_eq!(
        body["items"].as_array().expect("items array").len(),
        expected,
        "items: {body}"
    );
    assert_eq!(body["total"], expected as i64, "total: {body}");
}

#[tokio::test(flavor = "current_thread")]
async fn ots_recommendations_filter_by_assigned() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reader = app
        .create_user(
            10016301,
            "Ots Reader",
            &["training.ots_recommendations.read"],
        )
        .await;
    let instructor = app.create_user(10016302, "Ots Instructor", &[]).await;
    let assigned_student = app.create_user(10016303, "Ots Assigned", &[]).await;
    let unassigned_a = app.create_user(10016304, "Ots Unassigned A", &[]).await;
    let unassigned_b = app.create_user(10016305, "Ots Unassigned B", &[]).await;

    for (student, instructor_id) in [
        (&assigned_student, Some(instructor.id.as_str())),
        (&unassigned_a, None),
        (&unassigned_b, None),
    ] {
        sqlx::query(
            "insert into training.ots_recommendations (student_id, assigned_instructor_id, notes) values ($1, $2, 'n')",
        )
        .bind(&student.id)
        .bind(instructor_id)
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let all = get_list(&app, &reader, "/api/v1/training/ots-recommendations").await;
    assert_items_and_total(&all, 3);

    let assigned = get_list(
        &app,
        &reader,
        "/api/v1/training/ots-recommendations?assigned=true",
    )
    .await;
    assert_items_and_total(&assigned, 1);
    assert_eq!(assigned["items"][0]["student_id"], assigned_student.id);

    let unassigned = get_list(
        &app,
        &reader,
        "/api/v1/training/ots-recommendations?assigned=false",
    )
    .await;
    assert_items_and_total(&unassigned, 2);
    assert!(
        unassigned["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["assigned_instructor_id"].is_null())
    );

    // page_size=1 still reports the filtered total, which is how clients read counts.
    let count_only = get_list(
        &app,
        &reader,
        "/api/v1/training/ots-recommendations?assigned=false&page_size=1",
    )
    .await;
    assert_eq!(count_only["items"].as_array().unwrap().len(), 1);
    assert_eq!(count_only["total"], 2);

    let invalid = app
        .json_request(
            "GET",
            "/api/v1/training/ots-recommendations?assigned=maybe",
            Some(&reader.session_token),
            None,
        )
        .await;
    assert_status(&invalid, StatusCode::BAD_REQUEST);

    let no_permission = app.create_user(10016306, "Ots No Perm", &[]).await;
    let denied_response = app
        .json_request(
            "GET",
            "/api/v1/training/ots-recommendations?assigned=true",
            Some(&no_permission.session_token),
            None,
        )
        .await;
    assert_refused(&denied_response);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn assignment_requests_filter_by_student_controller_status() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reader = app
        .create_user(
            10016311,
            "Request Reader",
            &["training.assignment_requests.read"],
        )
        .await;
    let home_a = app.create_user(10016312, "Home Student A", &[]).await;
    let home_b = app.create_user(10016313, "Home Student B", &[]).await;
    let visitor = app.create_user(10016314, "Visitor Student", &[]).await;
    let none = app.create_user(10016315, "None Student", &[]).await;

    for (student, status) in [
        (&home_a, "HOME"),
        (&home_b, "HOME"),
        (&visitor, "VISITOR"),
        (&none, "NONE"),
    ] {
        sqlx::query("update org.memberships set controller_status = $2 where user_id = $1")
            .bind(&student.id)
            .bind(status)
            .execute(&app.pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into training.training_assignment_requests (student_id, submitted_at, status) values ($1, now(), 'PENDING')",
        )
        .bind(&student.id)
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let all = get_list(&app, &reader, "/api/v1/training/assignment-requests").await;
    assert_items_and_total(&all, 4);

    let home = get_list(
        &app,
        &reader,
        "/api/v1/training/assignment-requests?student_controller_status=HOME",
    )
    .await;
    assert_items_and_total(&home, 2);
    assert!(
        home["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["student_controller_status"] == "HOME")
    );

    let visitors = get_list(
        &app,
        &reader,
        "/api/v1/training/assignment-requests?student_controller_status=visitor",
    )
    .await;
    assert_items_and_total(&visitors, 1);
    assert_eq!(visitors["items"][0]["student_id"], visitor.id);

    for bad in ["NONE", "LOCAL", ""] {
        let invalid = app
            .json_request(
                "GET",
                &format!("/api/v1/training/assignment-requests?student_controller_status={bad}"),
                Some(&reader.session_token),
                None,
            )
            .await;
        assert_status(&invalid, StatusCode::BAD_REQUEST);
    }

    let no_permission = app.create_user(10016316, "Request No Perm", &[]).await;
    let denied_response = app
        .json_request(
            "GET",
            "/api/v1/training/assignment-requests?student_controller_status=HOME",
            Some(&no_permission.session_token),
            None,
        )
        .await;
    assert_refused(&denied_response);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn release_requests_filter_by_status() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reader = app
        .create_user(
            10016321,
            "Release Reader",
            &["training.release_requests.read"],
        )
        .await;
    let pending_a = app.create_user(10016322, "Release Pending A", &[]).await;
    let pending_b = app.create_user(10016323, "Release Pending B", &[]).await;
    let approved = app.create_user(10016324, "Release Approved", &[]).await;
    let denied = app.create_user(10016325, "Release Denied", &[]).await;

    for (student, status) in [
        (&pending_a, "PENDING"),
        (&pending_b, "PENDING"),
        (&approved, "APPROVED"),
        (&denied, "DENIED"),
    ] {
        sqlx::query(
            "insert into training.trainer_release_requests (student_id, submitted_at, status) values ($1, now(), $2)",
        )
        .bind(&student.id)
        .bind(status)
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let all = get_list(&app, &reader, "/api/v1/training/trainer-release-requests").await;
    assert_items_and_total(&all, 4);

    let pending = get_list(
        &app,
        &reader,
        "/api/v1/training/trainer-release-requests?status=pending",
    )
    .await;
    assert_items_and_total(&pending, 2);
    assert!(
        pending["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "PENDING")
    );

    let approved_list = get_list(
        &app,
        &reader,
        "/api/v1/training/trainer-release-requests?status=APPROVED",
    )
    .await;
    assert_items_and_total(&approved_list, 1);
    assert_eq!(approved_list["items"][0]["student_id"], approved.id);

    let denied_list = get_list(
        &app,
        &reader,
        "/api/v1/training/trainer-release-requests?status=DENIED",
    )
    .await;
    assert_items_and_total(&denied_list, 1);

    let invalid = app
        .json_request(
            "GET",
            "/api/v1/training/trainer-release-requests?status=CANCELLED",
            Some(&reader.session_token),
            None,
        )
        .await;
    assert_status(&invalid, StatusCode::BAD_REQUEST);

    let no_permission = app.create_user(10016326, "Release No Perm", &[]).await;
    let denied_response = app
        .json_request(
            "GET",
            "/api/v1/training/trainer-release-requests?status=PENDING",
            Some(&no_permission.session_token),
            None,
        )
        .await;
    assert_refused(&denied_response);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn appointments_filter_by_upcoming_and_double_booking() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let reader = app
        .create_user(
            10016331,
            "Appointment Reader",
            &["training.appointments.read"],
        )
        .await;
    let trainer = app.create_user(10016332, "Appointment Trainer", &[]).await;
    let other_trainer = app
        .create_user(10016333, "Appointment Other Trainer", &[])
        .await;
    let student = app.create_user(10016334, "Appointment Student", &[]).await;

    // (trainer, start offset from now, double_booking)
    let seeds = [
        (&trainer, "+2 days", false),
        (&trainer, "+3 days", true),
        (&other_trainer, "+4 days", true),
        (&trainer, "-2 days", false),
        (&trainer, "-3 days", true),
    ];
    for (trainer, offset, double_booking) in seeds {
        sqlx::query(
            "insert into training.training_appointments (student_id, trainer_id, start, double_booking) values ($1, $2, now() + $3::interval, $4)",
        )
        .bind(&student.id)
        .bind(&trainer.id)
        .bind(offset)
        .bind(double_booking)
        .execute(&app.pool)
        .await
        .unwrap();
    }

    let all = get_list(&app, &reader, "/api/v1/training/appointments").await;
    assert_items_and_total(&all, 5);

    let upcoming = get_list(&app, &reader, "/api/v1/training/appointments?upcoming=true").await;
    assert_items_and_total(&upcoming, 3);

    let past = get_list(
        &app,
        &reader,
        "/api/v1/training/appointments?upcoming=false",
    )
    .await;
    assert_items_and_total(&past, 2);

    let double_booked = get_list(
        &app,
        &reader,
        "/api/v1/training/appointments?double_booking=true",
    )
    .await;
    assert_items_and_total(&double_booked, 3);
    assert!(
        double_booked["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["double_booking"] == true)
    );

    let upcoming_double = get_list(
        &app,
        &reader,
        "/api/v1/training/appointments?upcoming=true&double_booking=true",
    )
    .await;
    assert_items_and_total(&upcoming_double, 2);

    // The new filters compose with the existing trainer filter rather than replacing it.
    let trainer_upcoming_double = get_list(
        &app,
        &reader,
        &format!(
            "/api/v1/training/appointments?upcoming=true&double_booking=true&trainer_id={}",
            trainer.id
        ),
    )
    .await;
    assert_items_and_total(&trainer_upcoming_double, 1);
    assert_eq!(
        trainer_upcoming_double["items"][0]["trainer_id"],
        trainer.id
    );

    let count_only = get_list(
        &app,
        &reader,
        "/api/v1/training/appointments?upcoming=true&page_size=1",
    )
    .await;
    assert_eq!(count_only["items"].as_array().unwrap().len(), 1);
    assert_eq!(count_only["total"], 3);

    let invalid = app
        .json_request(
            "GET",
            "/api/v1/training/appointments?upcoming=soon",
            Some(&reader.session_token),
            None,
        )
        .await;
    assert_status(&invalid, StatusCode::BAD_REQUEST);

    let no_permission = app.create_user(10016335, "Appointment No Perm", &[]).await;
    let denied_response = app
        .json_request(
            "GET",
            "/api/v1/training/appointments?upcoming=true",
            Some(&no_permission.session_token),
            None,
        )
        .await;
    assert_refused(&denied_response);

    app.cleanup().await;
}

/// `ensure_permission` answers 401 on master and 403 once #99 lands; either
/// way the caller is refused.
fn assert_refused(response: &axum::http::Response<axum::body::Body>) {
    assert!(
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ),
        "expected a refusal, got {}",
        response.status()
    );
}
