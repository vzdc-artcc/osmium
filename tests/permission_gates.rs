use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn admin_acl_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/acl")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_user_access_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/users/10000010/access")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_set_controller_status_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/admin/users/10000010/controller-status")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"controller_status\":\"VISITOR\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_refresh_user_vatusa_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users/10000010/refresh-vatusa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_list_visitor_applications_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/visitor-applications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_decide_visitor_application_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/admin/visitor-applications/test-application-id")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"status\":\"APPROVED\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_access_catalog_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/access/catalog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_list_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_lookup_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/10000010")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_visit_artcc_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/visit-artcc")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"artcc\":\"ZDC\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_refresh_vatusa_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/refresh-vatusa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_get_visitor_application_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/visitor-application")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_create_visitor_application_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/visitor-application")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"home_facility\":\"ZNY\",\"why_visit\":\"Interested in events\"}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_feedback_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/10000010/feedback?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn patch_me_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/v1/me")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"preferred_name\":\"Jay\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_teamspeak_uids_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/me/teamspeak-uids")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_teamspeak_uid_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/me/teamspeak-uids")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"uid\":\"AbCdEf123\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_teamspeak_uid_endpoint_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/me/teamspeak-uids/test-identity-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn artcc_stats_endpoint_is_public_by_policy() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/stats/artcc?month=3&year=2026")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn controller_history_stats_endpoint_is_public_by_policy() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/stats/controller/10000010/history?year=2026")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn controller_totals_stats_endpoint_is_public_by_policy() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/stats/controller/10000010/totals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn training_assignments_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/assignments")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_ots_recommendations_list_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/ots-recommendations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_ots_recommendations_create_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/ots-recommendations")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"student_id\":\"user-1\",\"notes\":\"Ready for OTS\"}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_ots_recommendations_update_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/training/ots-recommendations/ots-1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"assigned_instructor_id\":\"user-2\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_ots_recommendations_delete_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/training/ots-recommendations/ots-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_lessons_endpoint_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/lessons")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_lesson_create_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/lessons")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"identifier\":\"OBS1\",\"location\":0,\"name\":\"Intro\",\"description\":\"Desc\",\"position\":\"DCA_DEL\",\"facility\":\"DCA\",\"duration\":60,\"instructor_only\":false,\"notify_instructor_on_pass\":false,\"release_request_on_pass\":false}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_lesson_update_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/training/lessons/lesson-1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"identifier\":\"OBS1\",\"location\":0,\"name\":\"Intro\",\"description\":\"Desc\",\"position\":\"DCA_DEL\",\"facility\":\"DCA\",\"duration\":60,\"instructor_only\":false,\"notify_instructor_on_pass\":false,\"release_request_on_pass\":false}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_lesson_delete_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/training/lessons/lesson-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_appointments_list_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/appointments")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_appointment_detail_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/appointments/appointment-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_appointment_create_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/appointments")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"student_id\":\"user-1\",\"start\":\"2026-05-04T20:02:00Z\",\"lesson_ids\":[\"lesson-1\"]}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_appointment_update_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/training/appointments/appointment-1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"student_id\":\"user-1\",\"start\":\"2026-05-04T20:02:00Z\",\"lesson_ids\":[\"lesson-1\"]}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_appointment_delete_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/training/appointments/appointment-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_sessions_list_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_session_detail_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/training/sessions/session-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_session_create_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/sessions")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"student_id\":\"user-1\",\"start\":\"2026-04-28T12:00:00Z\",\"end\":\"2026-04-28T13:00:00Z\",\"tickets\":[]}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_session_update_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/training/sessions/session-1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"student_id\":\"user-1\",\"start\":\"2026-04-28T12:00:00Z\",\"end\":\"2026-04-28T13:00:00Z\",\"tickets\":[]}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_session_delete_requires_staff_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/training/sessions/session-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_assignment_request_create_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/assignment-requests")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn training_assignment_request_interest_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/training/assignment-requests/request-1/interest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn feedback_create_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/feedback")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"target_cid\":10000010,\"pilot_callsign\":\"N123AB\",\"controller_position\":\"DCA_TWR\",\"rating\":4}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn feedback_list_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/feedback")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn feedback_decide_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/feedback/fb-1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"status\":\"RELEASED\"}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn event_position_create_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/events/event-1/positions")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    "{\"callsign\":\"DCA_DEL\",\"requested_slot\":1}",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn files_list_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/files")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn files_download_requires_db_when_not_configured() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/files/file-1/content")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn files_signed_url_requires_session() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/files/file-1/signed-url")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cdn_route_requires_db_when_not_configured() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/cdn/file-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
