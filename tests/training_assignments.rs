mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn assignment_crud_lifecycle_works_end_to_end() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000071,
            "Assignment Staff",
            &[
                "training.assignments.create",
                "training.assignments.read",
                "training.assignments.update",
                "training.assignments.delete",
            ],
        )
        .await;
    let student = app.create_user(10000072, "Assignment Student", &[]).await;
    let trainer_a = app.create_user(10000073, "Trainer A", &[]).await;
    let trainer_b = app.create_user(10000074, "Trainer B", &[]).await;
    let trainer_c = app.create_user(10000075, "Trainer C", &[]).await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignments",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "primary_trainer_id": trainer_a.id,
                "other_trainer_ids": [trainer_b.id]
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let assignment_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["primary_trainer_id"], trainer_a.id);
    assert_eq!(
        created["other_trainer_ids"].as_array().unwrap(),
        &vec![Value::String(trainer_b.id.clone())]
    );
    assert_eq!(created["student_cid"], 10000072);
    assert_eq!(created["student_name"], "Assignment Student");
    assert_eq!(created["primary_trainer_cid"], 10000073);
    assert_eq!(created["primary_trainer_name"], "Trainer A");
    let other_trainers = created["other_trainers"].as_array().unwrap();
    assert_eq!(other_trainers.len(), 1);
    assert_eq!(other_trainers[0]["id"], trainer_b.id);
    assert_eq!(other_trainers[0]["cid"], 10000074);
    assert_eq!(other_trainers[0]["name"], "Trainer B");

    let get_response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&get_response, StatusCode::OK);

    // Reassigning to a primary trainer who's also listed as an "other" trainer is rejected.
    let bad_update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            Some(json!({
                "primary_trainer_id": trainer_b.id,
                "other_trainer_ids": [trainer_b.id]
            })),
        )
        .await;
    assert_status(&bad_update_response, StatusCode::BAD_REQUEST);

    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            Some(json!({
                "primary_trainer_id": trainer_c.id,
                "other_trainer_ids": [trainer_a.id, trainer_b.id]
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let updated: Value = json_body(update_response).await;
    assert_eq!(updated["primary_trainer_id"], trainer_c.id);
    let mut other_ids: Vec<String> = updated["other_trainer_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    other_ids.sort();
    let mut expected = vec![trainer_a.id.clone(), trainer_b.id.clone()];
    expected.sort();
    assert_eq!(other_ids, expected);

    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::NO_CONTENT);

    let get_after_delete_response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&get_after_delete_response, StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn assignment_request_self_cancel_and_admin_delete_work() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000076,
            "Request Staff",
            &["training.assignment_requests.delete"],
        )
        .await;
    let student = app
        .create_user(
            10000077,
            "Request Student",
            &["training.assignment_requests.self.request"],
        )
        .await;
    let bystander = app.create_user(10000078, "Request Bystander", &[]).await;

    // Self-cancel: a student can delete their own PENDING request without any extra permission.
    let create_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&student.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let request_id = created["id"].as_str().unwrap().to_string();

    let self_cancel_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/assignment-requests/{request_id}"),
            Some(&student.session_token),
            None,
        )
        .await;
    assert_status(&self_cancel_response, StatusCode::NO_CONTENT);

    // A bystander with no permission of their own cannot delete someone else's request...
    let create_response_2 = app
        .json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&student.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&create_response_2, StatusCode::CREATED);
    let created_2: Value = json_body(create_response_2).await;
    let request_id_2 = created_2["id"].as_str().unwrap().to_string();

    let unauthorized_delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/assignment-requests/{request_id_2}"),
            Some(&bystander.session_token),
            None,
        )
        .await;
    assert_status(&unauthorized_delete_response, StatusCode::UNAUTHORIZED);

    // ...but staff holding the admin delete permission can.
    let admin_delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/assignment-requests/{request_id_2}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&admin_delete_response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn assignment_request_interest_is_listed_with_trainer_details() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10000081, "Interest Staff", &["training.assignment_requests.read"])
        .await;
    let student = app
        .create_user(
            10000082,
            "Interest Student",
            &["training.assignment_requests.self.request"],
        )
        .await;
    let trainer = app
        .create_user(
            10000083,
            "Interested Trainer",
            &[
                "training.assignment_requests.interest.request",
                "training.assignment_requests.interest.delete",
            ],
        )
        .await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&student.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let request_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["student_cid"], 10000082);
    assert_eq!(created["student_name"], "Interest Student");
    assert_eq!(created["interested_trainers"].as_array().unwrap().len(), 0);

    let express_interest_response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/assignment-requests/{request_id}/interest"),
            Some(&trainer.session_token),
            None,
        )
        .await;
    assert_status(&express_interest_response, StatusCode::NO_CONTENT);

    let list_response = app
        .json_request(
            "GET",
            "/api/v1/training/assignment-requests?page_size=200",
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
        .find(|item| item["id"] == request_id)
        .expect("request present in list");
    let interested = found["interested_trainers"].as_array().unwrap();
    assert_eq!(interested.len(), 1);
    assert_eq!(interested[0]["id"], trainer.id);
    assert_eq!(interested[0]["cid"], 10000083);
    assert_eq!(interested[0]["name"], "Interested Trainer");

    let remove_interest_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/assignment-requests/{request_id}/interest"),
            Some(&trainer.session_token),
            None,
        )
        .await;
    assert_status(&remove_interest_response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn assignment_request_manual_creation_requires_create_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10000084, "Manual Staff", &["training.assignment_requests.create"])
        .await;
    let bystander = app.create_user(10000085, "Manual Bystander", &[]).await;
    let student = app.create_user(10000086, "Manual Student", &[]).await;

    // A caller without the create permission cannot submit on someone else's behalf.
    let unauthorized_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&bystander.session_token),
            Some(json!({"student_id": student.id})),
        )
        .await;
    assert_status(&unauthorized_response, StatusCode::UNAUTHORIZED);

    // Staff holding the create permission can, and can backdate submitted_at.
    let backdated = "2024-01-15T12:00:00Z";
    let manual_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&staff.session_token),
            Some(json!({"student_id": student.id, "submitted_at": backdated})),
        )
        .await;
    assert_status(&manual_response, StatusCode::CREATED);
    let created: Value = json_body(manual_response).await;
    assert_eq!(created["student_id"], student.id);
    assert_eq!(created["student_cid"], 10000086);
    let submitted_at: chrono::DateTime<chrono::Utc> =
        created["submitted_at"].as_str().unwrap().parse().unwrap();
    assert_eq!(submitted_at, backdated.parse::<chrono::DateTime<chrono::Utc>>().unwrap());

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn approving_release_request_ends_the_assignment() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000078,
            "Release Staff",
            &[
                "training.assignments.create",
                "training.assignments.read",
                "training.release_requests.decide",
            ],
        )
        .await;
    let student = app
        .create_user(
            10000079,
            "Release Student",
            &["training.release_requests.self.request"],
        )
        .await;
    let trainer = app.create_user(10000080, "Release Trainer", &[]).await;

    let create_assignment_response = app
        .json_request(
            "POST",
            "/api/v1/training/assignments",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "primary_trainer_id": trainer.id
            })),
        )
        .await;
    assert_status(&create_assignment_response, StatusCode::CREATED);
    let assignment: Value = json_body(create_assignment_response).await;
    let assignment_id = assignment["id"].as_str().unwrap().to_string();

    let create_release_response = app
        .json_request(
            "POST",
            "/api/v1/training/trainer-release-requests",
            Some(&student.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&create_release_response, StatusCode::CREATED);
    let release_request: Value = json_body(create_release_response).await;
    let release_request_id = release_request["id"].as_str().unwrap().to_string();

    let approve_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/trainer-release-requests/{release_request_id}"),
            Some(&staff.session_token),
            Some(json!({"status": "APPROVED"})),
        )
        .await;
    assert_status(&approve_response, StatusCode::OK);

    let get_assignment_response = app
        .json_request(
            "GET",
            &format!("/api/v1/training/assignments/{assignment_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&get_assignment_response, StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn release_request_manual_creation_requires_create_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10000087, "Release Manual Staff", &["training.release_requests.create"])
        .await;
    let trainer = app
        .create_user(10000088, "Release Manual Trainer", &["training.release_requests.self.request"])
        .await;
    let student = app.create_user(10000089, "Release Manual Student", &[]).await;

    // A caller without the create permission cannot submit release on someone else's behalf,
    // even if they hold the self-request permission for their own release requests.
    let unauthorized_response = app
        .json_request(
            "POST",
            "/api/v1/training/trainer-release-requests",
            Some(&trainer.session_token),
            Some(json!({"student_id": student.id})),
        )
        .await;
    assert_status(&unauthorized_response, StatusCode::UNAUTHORIZED);

    // The same trainer can still submit for themselves (self-request permission only).
    let self_response = app
        .json_request(
            "POST",
            "/api/v1/training/trainer-release-requests",
            Some(&trainer.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&self_response, StatusCode::CREATED);
    let self_created: Value = json_body(self_response).await;
    assert_eq!(self_created["student_id"], trainer.id);

    // Staff holding the create permission can submit on the student's behalf.
    let manual_response = app
        .json_request(
            "POST",
            "/api/v1/training/trainer-release-requests",
            Some(&staff.session_token),
            Some(json!({"student_id": student.id})),
        )
        .await;
    assert_status(&manual_response, StatusCode::CREATED);
    let created: Value = json_body(manual_response).await;
    assert_eq!(created["student_id"], student.id);
    assert_eq!(created["student_cid"], 10000089);

    app.cleanup().await;
}
