mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn session_crud_lifecycle_and_pi_full_replace_contract() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000094,
            "Session Staff",
            &[
                "training.sessions.create",
                "training.sessions.read",
                "training.sessions.update",
                "training.sessions.delete",
                "training.lessons.create",
                "training.lessons.update",
                "users.directory.read",
            ],
        )
        .await;
    let student = app.create_user(10000095, "Session Student", &[]).await;

    // Performance indicator template, referenced by the lesson.
    let create_template_response = app
        .json_request(
            "POST",
            "/api/v1/admin/training/performance-indicators/templates",
            Some(&staff.session_token),
            Some(json!({"name": "Radar Basics PI"})),
        )
        .await;
    assert_status(&create_template_response, StatusCode::CREATED);
    let template: Value = json_body(create_template_response).await;
    let template_id = template["id"].as_str().unwrap().to_string();

    // Lesson with a rubric and the PI template attached.
    let create_lesson_response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            Some(&staff.session_token),
            Some(json!({
                "identifier": "DEL1",
                "location": 2,
                "name": "Basic Radar",
                "description": "desc",
                "position": "DEL",
                "facility": "PCT",
                "duration": 60,
                "trainee_preparation": null,
                "instructor_only": false,
                "notify_instructor_on_pass": false,
                "release_request_on_pass": false,
                "performance_indicator_template_id": template_id
            })),
        )
        .await;
    assert_status(&create_lesson_response, StatusCode::CREATED);
    let lesson: Value = json_body(create_lesson_response).await;
    let lesson_id = lesson["id"].as_str().unwrap().to_string();

    let create_criteria_response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria"),
            Some(&staff.session_token),
            Some(json!({
                "criteria": "Radar Identification",
                "description": "desc",
                "max_points": 3,
                "passing": 2
            })),
        )
        .await;
    assert_status(&create_criteria_response, StatusCode::CREATED);
    let criteria: Value = json_body(create_criteria_response).await;
    let criteria_id = criteria["id"].as_str().unwrap().to_string();

    let create_cell_response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells"),
            Some(&staff.session_token),
            Some(json!({"points": 3, "description": "Perfect"})),
        )
        .await;
    assert_status(&create_cell_response, StatusCode::CREATED);
    let cell: Value = json_body(create_cell_response).await;
    let cell_id = cell["id"].as_str().unwrap().to_string();

    // Create the session with a passing ticket and a PI snapshot.
    let create_session_response = app
        .json_request(
            "POST",
            "/api/v1/training/sessions",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:00:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "performance_indicator": {
                    "categories": [{
                        "name": "Basics",
                        "order": 1,
                        "criteria": [{
                            "name": "Scan",
                            "order": 1,
                            "marker": "OBSERVED",
                            "comments": null
                        }]
                    }]
                },
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&create_session_response, StatusCode::CREATED);
    let created: Value = json_body(create_session_response).await;
    let session_id = created["session"]["id"].as_str().unwrap().to_string();
    assert!(created["session"]["performance_indicator"].is_object());
    assert_eq!(
        created["session"]["performance_indicator"]["categories"][0]["criteria"][0]["marker"],
        "OBSERVED"
    );

    // The lesson requires a PI, so an update must resend it to pass validation
    // (PI follows the same "client resends what it wants to keep" full-replace
    // contract as tickets/additional_trainers) — resending the same content
    // must round-trip unchanged.
    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": "UPDATED",
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "performance_indicator": {
                    "categories": [{
                        "name": "Basics",
                        "order": 1,
                        "criteria": [{
                            "name": "Scan",
                            "order": 1,
                            "marker": "OBSERVED",
                            "comments": null
                        }]
                    }]
                },
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let updated: Value = json_body(update_response).await;
    assert_eq!(updated["session"]["additional_comments"], "UPDATED");
    assert!(
        updated["session"]["performance_indicator"].is_object(),
        "performance indicator must round-trip when resent unchanged on update, got: {updated:?}"
    );
    assert_eq!(
        updated["session"]["performance_indicator"]["categories"][0]["criteria"][0]["marker"],
        "OBSERVED"
    );

    // A lesson requiring PI rejects an update that omits it entirely (full-replace
    // contract — the caller must resend what it wants to keep).
    let missing_pi_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": "UPDATED AGAIN",
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&missing_pi_response, StatusCode::BAD_REQUEST);
    let missing_pi_body: Value = json_body(missing_pi_response).await;
    assert!(!missing_pi_body["errors"].as_array().unwrap().is_empty());

    // Update-path validation errors must surface a populated `errors` body, same as create.
    let invalid_update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [],
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&invalid_update_response, StatusCode::BAD_REQUEST);
    let invalid_body: Value = json_body(invalid_update_response).await;
    assert!(
        !invalid_body["errors"].as_array().unwrap().is_empty(),
        "expected populated errors array on update validation failure, got: {invalid_body:?}"
    );

    // List item ticket denormalization.
    let list_response = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&list_response, StatusCode::OK);
    let list: Value = json_body(list_response).await;
    let found = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == session_id)
        .expect("session present in list");
    assert_eq!(found["ticket_count"], 1);
    assert_eq!(found["tickets"][0]["lesson_identifier"], "DEL1");
    assert_eq!(found["tickets"][0]["passed"], true);

    // Roster list exposes role_names/operating_initials to a privileged caller.
    let roster_response = app
        .json_request(
            "GET",
            "/api/v1/users?page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&roster_response, StatusCode::OK);
    let roster: Value = json_body(roster_response).await;
    let staff_row = roster["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["basic"]["cid"] == 10000094)
        .expect("staff present in roster");
    assert!(staff_row["full"]["role_names"].is_array());
    assert!(
        staff_row["full"]
            .as_object()
            .unwrap()
            .contains_key("operating_initials")
    );

    // Cleanup.
    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}

/// Regression: `sort_field=end` must produce valid SQL. `end` is a reserved SQL
/// keyword, so the ORDER BY column has to be quoted (`ts."end"`); the unquoted
/// `ts.end` it once used is a syntax error that made the whole list endpoint 500.
/// This exercises the actual query end-to-end so a later refactor of the
/// `sort_column` match that drops the quoting is caught.
#[tokio::test(flavor = "current_thread")]
async fn list_sessions_sorts_by_end_keyword() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000200,
            "Sort Staff",
            &["training.sessions.read", "users.directory.read"],
        )
        .await;
    let student = app.create_user(10000201, "Sort Student", &[]).await;

    // Two sessions whose end-order differs from their start-order, so sorting by
    // `end` produces an order distinct from the default start sort.
    for (id, start, end) in [
        (
            "session-end-a",
            "2026-03-01T10:00:00Z",
            "2026-03-01T12:00:00Z",
        ),
        (
            "session-end-b",
            "2026-03-01T11:00:00Z",
            "2026-03-01T11:30:00Z",
        ),
    ] {
        sqlx::query(
            r#"
            insert into training.training_sessions (id, student_id, instructor_id, start, "end")
            values ($1, $2, $3, $4::timestamptz, $5::timestamptz)
            "#,
        )
        .bind(id)
        .bind(&student.id)
        .bind(&staff.id)
        .bind(start)
        .bind(end)
        .execute(&app.pool)
        .await
        .expect("seed training session");
    }

    // Ascending by end: B (11:30) before A (12:00). A 500 here would mean the
    // reserved-keyword quoting regressed.
    let asc = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?sort_field=end&sort_order=asc&page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&asc, StatusCode::OK);
    let asc: Value = json_body(asc).await;
    let asc_ids: Vec<&str> = asc["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        asc_ids,
        vec!["session-end-b", "session-end-a"],
        "sessions must be ordered by ascending end time"
    );

    // Descending by end: A (12:00) before B (11:30).
    let desc = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?sort_field=end&sort_order=desc&page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&desc, StatusCode::OK);
    let desc: Value = json_body(desc).await;
    let desc_ids: Vec<&str> = desc["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        desc_ids,
        vec!["session-end-a", "session-end-b"],
        "sessions must be ordered by descending end time"
    );

    app.cleanup().await;
}

/// A session records the performance indicator as assessed at the time:
/// renaming the template's categories and criteria, or deleting them and the
/// template, must not change or remove what a past session shows.
#[tokio::test(flavor = "current_thread")]
async fn session_performance_indicator_survives_template_edits_and_deletes() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000098,
            "PI Staff",
            &[
                "training.sessions.create",
                "training.sessions.read",
                "training.lessons.read",
                "training.lessons.create",
                "training.lessons.update",
                "users.directory.read",
            ],
        )
        .await;
    let student = app.create_user(10000099, "PI Student", &[]).await;
    let admin = "/api/v1/admin/training/performance-indicators";

    let response = app
        .json_request(
            "POST",
            &format!("{admin}/templates"),
            Some(&staff.session_token),
            Some(json!({"name": "Ground PI"})),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let template_id = json_body::<Value>(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .json_request(
            "POST",
            &format!("{admin}/categories"),
            Some(&staff.session_token),
            Some(json!({"template_id": template_id, "name": "Basics", "sort_order": 1})),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let category_id = json_body::<Value>(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .json_request(
            "POST",
            &format!("{admin}/criteria"),
            Some(&staff.session_token),
            Some(json!({"category_id": category_id, "name": "Scan", "sort_order": 1})),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let criteria_id = json_body::<Value>(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            Some(&staff.session_token),
            Some(json!({
                "identifier": "GND1",
                "location": 2,
                "name": "Ground Basics",
                "description": "desc",
                "position": "GND",
                "facility": "DCA",
                "duration": 60,
                "trainee_preparation": null,
                "instructor_only": false,
                "notify_instructor_on_pass": false,
                "release_request_on_pass": false,
                "performance_indicator_template_id": template_id
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let lesson_id = json_body::<Value>(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .json_request(
            "POST",
            "/api/v1/training/sessions",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:00:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{"lesson_id": lesson_id, "passed": true, "scores": []}],
                "performance_indicator": {
                    "categories": [{
                        "name": "Basics",
                        "order": 1,
                        "criteria": [{"name": "Scan", "order": 1, "marker": "OBSERVED", "comments": null}]
                    }]
                },
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let session_id = json_body::<Value>(response).await["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Rename, then delete, every part of the template.
    for (path, body) in [
        (
            format!("{admin}/categories/{category_id}"),
            json!({"name": "Renamed category"}),
        ),
        (
            format!("{admin}/criteria/{criteria_id}"),
            json!({"name": "Renamed criterion"}),
        ),
    ] {
        let response = app
            .json_request("PATCH", &path, Some(&staff.session_token), Some(body))
            .await;
        assert_status(&response, StatusCode::OK);
    }
    for path in [
        format!("{admin}/criteria/{criteria_id}"),
        format!("{admin}/categories/{category_id}"),
        format!("{admin}/templates/{template_id}"),
    ] {
        let response = app
            .json_request("DELETE", &path, Some(&staff.session_token), None)
            .await;
        assert!(
            response.status().is_success(),
            "DELETE {path} returned {}",
            response.status()
        );
    }

    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let session = if body["session"].is_object() {
        &body["session"]
    } else {
        &body
    };
    let category = &session["performance_indicator"]["categories"][0];
    assert_eq!(category["name"], "Basics", "session indicator: {session}");
    assert_eq!(category["criteria"][0]["name"], "Scan");
    assert_eq!(category["criteria"][0]["marker"], "OBSERVED");

    app.cleanup().await;
}

/// A session records its rubric scores as assessed at the time: editing a
/// criterion or cell, or deleting them, must not change or remove what a past
/// session shows it was scored against.
#[tokio::test(flavor = "current_thread")]
async fn session_rubric_scores_survive_rubric_edits_and_deletes() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10001190,
            "Rubric Staff",
            &[
                "training.sessions.create",
                "training.sessions.read",
                "training.sessions.update",
                "training.lessons.read",
                "training.lessons.create",
                "training.lessons.update",
                "training.lessons.delete",
                "users.directory.read",
            ],
        )
        .await;
    let student = app.create_user(10001191, "Rubric Student", &[]).await;
    let token = Some(staff.session_token.as_str());

    let response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            token,
            Some(json!({
                "identifier": "TWR1",
                "location": 2,
                "name": "Tower Basics",
                "description": "desc",
                "position": "TWR",
                "facility": "DCA",
                "duration": 60,
                "trainee_preparation": null,
                "instructor_only": false,
                "notify_instructor_on_pass": false,
                "release_request_on_pass": false,
                "performance_indicator_template_id": null
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let lesson_id = json_body::<Value>(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let criteria_path = format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria");

    // Two criteria, each with a cell to score and a second cell left alone.
    let mut criteria_ids = Vec::new();
    let mut scored_cell_ids = Vec::new();
    for (name, description, max_points, passing, cells) in [
        (
            "Phraseology",
            "Uses standard phraseology",
            4,
            3,
            [(4, "Always standard"), (1, "Rarely standard")],
        ),
        (
            "Separation",
            "Maintains runway separation",
            5,
            4,
            [(5, "No losses"), (0, "Loss of separation")],
        ),
    ] {
        let response = app
            .json_request(
                "POST",
                &criteria_path,
                token,
                Some(json!({
                    "criteria": name,
                    "description": description,
                    "max_points": max_points,
                    "passing": passing
                })),
            )
            .await;
        assert_status(&response, StatusCode::CREATED);
        let criteria_id = json_body::<Value>(response).await["id"]
            .as_str()
            .unwrap()
            .to_string();
        for (index, (points, cell_description)) in cells.into_iter().enumerate() {
            let response = app
                .json_request(
                    "POST",
                    &format!("{criteria_path}/{criteria_id}/cells"),
                    token,
                    Some(json!({"points": points, "description": cell_description})),
                )
                .await;
            assert_status(&response, StatusCode::CREATED);
            let cell_id = json_body::<Value>(response).await["id"]
                .as_str()
                .unwrap()
                .to_string();
            if index == 0 {
                scored_cell_ids.push(cell_id);
            }
        }
        criteria_ids.push(criteria_id);
    }

    let response = app
        .json_request(
            "POST",
            "/api/v1/training/sessions",
            token,
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:00:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [
                        {"criteria_id": criteria_ids[0], "cell_id": scored_cell_ids[0], "passed": true},
                        {"criteria_id": criteria_ids[1], "cell_id": scored_cell_ids[1], "passed": true}
                    ]
                }],
                "performance_indicator": null,
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let session_id = json_body::<Value>(response).await["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Edit the first criterion and its scored cell.
    let response = app
        .json_request(
            "PATCH",
            &format!("{criteria_path}/{}", criteria_ids[0]),
            token,
            Some(json!({
                "criteria": "Renamed criterion",
                "description": "Renamed description",
                "max_points": 6,
                "passing": 5
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let response = app
        .json_request(
            "PATCH",
            &format!(
                "{criteria_path}/{}/cells/{}",
                criteria_ids[0], scored_cell_ids[0]
            ),
            token,
            Some(json!({"points": 2, "description": "Renamed cell"})),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    // Positive control: the live rubric shows the edit.
    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric"),
            token,
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let rubric: Value = json_body(response).await;
    let live = rubric["criteria"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == criteria_ids[0].as_str())
        .expect("edited criterion in live rubric");
    assert_eq!(
        live["criteria"], "Renamed criterion",
        "live rubric: {rubric}"
    );

    // Delete the second criterion's scored cell on its own, then the criterion.
    for path in [
        format!(
            "{criteria_path}/{}/cells/{}",
            criteria_ids[1], scored_cell_ids[1]
        ),
        format!("{criteria_path}/{}", criteria_ids[1]),
    ] {
        let response = app.json_request("DELETE", &path, token, None).await;
        assert!(
            response.status().is_success(),
            "DELETE {path} returned {}",
            response.status()
        );
    }
    let remaining: i64 = sqlx::query_scalar(
        "select count(*)::bigint from training.lesson_rubric_criteria where id = $1",
    )
    .bind(&criteria_ids[1])
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0, "second criterion was deleted");

    let stored: i64 = sqlx::query_scalar(
        r#"
        select count(*)::bigint
        from training.rubric_scores s
        join training.training_tickets t on t.id = s.training_ticket_id
        where t.session_id = $1
        "#,
    )
    .bind(&session_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(stored, 2, "both score rows survive");

    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/sessions/{session_id}"),
            token,
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let session = if body["session"].is_object() {
        &body["session"]
    } else {
        &body
    };
    let scores = session["tickets"][0]["scores"].as_array().unwrap();
    assert_eq!(scores.len(), 2, "session: {session}");
    let score_for = |criteria_id: &str| {
        scores
            .iter()
            .find(|s| s["criteria_id"] == criteria_id)
            .unwrap_or_else(|| panic!("score for {criteria_id} in {session}"))
    };

    let edited = score_for(&criteria_ids[0]);
    assert_eq!(edited["cell_id"], scored_cell_ids[0].as_str());
    assert_eq!(edited["criteria"], "Phraseology");
    assert_eq!(edited["criteria_description"], "Uses standard phraseology");
    assert_eq!(edited["max_points"], 4);
    assert_eq!(edited["passing"], 3);
    assert_eq!(edited["cell_points"], 4);
    assert_eq!(edited["cell_description"], "Always standard");

    let deleted = score_for(&criteria_ids[1]);
    assert_eq!(deleted["cell_id"], scored_cell_ids[1].as_str());
    assert_eq!(deleted["passed"], true);
    assert_eq!(deleted["criteria"], "Separation");
    assert_eq!(
        deleted["criteria_description"],
        "Maintains runway separation"
    );
    assert_eq!(deleted["max_points"], 5);
    assert_eq!(deleted["passing"], 4);
    assert_eq!(deleted["cell_points"], 5);
    assert_eq!(deleted["cell_description"], "No losses");

    // Saving the session again takes a fresh copy from the rubric as it now
    // stands: the renamed criterion and the edited cell's new points.
    let response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            token,
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:00:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [
                        {"criteria_id": criteria_ids[0], "cell_id": scored_cell_ids[0], "passed": true}
                    ]
                }],
                "performance_indicator": null,
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/sessions/{session_id}"),
            token,
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let session = if body["session"].is_object() {
        &body["session"]
    } else {
        &body
    };
    let scores = session["tickets"][0]["scores"].as_array().unwrap();
    assert_eq!(scores.len(), 1, "re-saved session: {session}");
    let resaved = &scores[0];
    assert_eq!(resaved["criteria_id"], criteria_ids[0].as_str());
    assert_eq!(resaved["criteria"], "Renamed criterion");
    assert_eq!(resaved["criteria_description"], "Renamed description");
    assert_eq!(resaved["max_points"], 6);
    assert_eq!(resaved["passing"], 5);
    assert_eq!(resaved["cell_points"], 2);
    assert_eq!(resaved["cell_description"], "Renamed cell");

    app.cleanup().await;
}

/// A score insert copies its criterion and cell from the live rubric. A pair
/// that does not resolve to a criterion and one of its own cells writes
/// nothing and reports a conflict.
#[tokio::test(flavor = "current_thread")]
async fn rubric_score_insert_refuses_an_unresolved_criterion_and_cell() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let instructor = app.create_user(10001192, "Rubric Instructor", &[]).await;
    let student = app.create_user(10001193, "Rubric Trainee", &[]).await;
    sqlx::raw_sql(
        r#"
        insert into training.lesson_rubrics (id) values ('r1');
        insert into training.lessons (id, identifier, location, name, description, position, facility, rubric_id)
        values ('l1', 'TWR1', 2, 'Tower', 'desc', 'TWR', 'DCA', 'r1');
        insert into training.lesson_rubric_criteria (id, rubric_id, criteria, description, passing, max_points)
        values ('c1', 'r1', 'Phraseology', 'desc', 3, 4), ('c2', 'r1', 'Separation', 'desc', 4, 5);
        insert into training.lesson_rubric_cells (id, criteria_id, points, description)
        values ('x1', 'c1', 4, 'Always standard'), ('x2', 'c2', 5, 'No losses');
        "#,
    )
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into training.training_sessions (id, student_id, instructor_id, start, "end")
        values ('s1', $1, $2, now(), now())
        "#,
    )
    .bind(&student.id)
    .bind(&instructor.id)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into training.training_tickets (id, session_id, lesson_id, passed) values ('t1', 's1', 'l1', true)",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let mut tx = app.pool.begin().await.unwrap();
    for (score_id, criteria_id, cell_id, case) in [
        (
            "score-missing",
            "missing-criteria",
            "missing-cell",
            "both missing",
        ),
        ("score-foreign", "c1", "x2", "cell of another criterion"),
    ] {
        let result = osmium::repos::training::sessions::insert_rubric_score_row(
            &mut tx,
            score_id,
            "t1",
            criteria_id,
            cell_id,
            true,
        )
        .await;
        assert!(
            matches!(result, Err(osmium::errors::ApiError::Conflict)),
            "{case}: expected Conflict, got {result:?}"
        );
    }
    let stored: i64 = sqlx::query_scalar("select count(*)::bigint from training.rubric_scores")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(stored, 0, "no row for an unresolved pair");

    // Positive control: the criterion with its own cell inserts one copy.
    osmium::repos::training::sessions::insert_rubric_score_row(
        &mut tx, "score-ok", "t1", "c1", "x1", true,
    )
    .await
    .expect("criterion with its own cell inserts");
    let (criteria, cell_points): (String, i32) = sqlx::query_as(
        "select criteria, cell_points from training.rubric_scores where id = 'score-ok'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!((criteria.as_str(), cell_points), ("Phraseology", 4));
    tx.rollback().await.unwrap();

    app.cleanup().await;
}
