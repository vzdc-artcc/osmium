mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn appointment_crud_lifecycle_includes_denormalized_lessons() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000090,
            "Appointment Staff",
            &[
                "training.appointments.create",
                "training.appointments.read",
                "training.appointments.update",
                "training.appointments.delete",
                "training.lessons.create",
            ],
        )
        .await;
    let student = app.create_user(10000091, "Appointment Student", &[]).await;
    let other_trainer = app
        .create_user(10000093, "Appointment Other Trainer", &[])
        .await;

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
                "performance_indicator_template_id": null
            })),
        )
        .await;
    assert_status(&create_lesson_response, StatusCode::CREATED);
    let lesson: Value = json_body(create_lesson_response).await;
    let lesson_id = lesson["id"].as_str().unwrap().to_string();

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/training/appointments",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "lesson_ids": [lesson_id],
                "environment": "SWEATBOX1",
                "notes": "",
                "additional_trainers": [{"trainer_id": other_trainer.id, "description": "OBSERVING"}]
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let appointment_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["student_cid"], 10000091);
    assert_eq!(created["lessons"][0]["identifier"], "DEL1");
    assert_eq!(created["additional_trainers"][0]["trainer_cid"], 10000093);

    let list_response = app
        .json_request(
            "GET",
            "/api/v1/training/appointments?page_size=200",
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
        .find(|item| item["id"] == appointment_id)
        .expect("appointment present in list");
    assert_eq!(found["lesson_count"], 1);
    assert_eq!(found["lessons"][0]["identifier"], "DEL1");
    assert_eq!(found["estimated_duration_minutes"], 60);
    assert_eq!(found["additional_trainer_count"], 1);
    assert_eq!(found["additional_trainers"][0]["trainer_cid"], 10000093);

    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/appointments/{appointment_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T13:00:00Z",
                "lesson_ids": [lesson_id],
                "notes": "UPDATED"
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let updated: Value = json_body(update_response).await;
    assert_eq!(updated["notes"], "UPDATED");

    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/appointments/{appointment_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}
