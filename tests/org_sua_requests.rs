mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn sua_request_self_service_lifecycle_and_validation_work_end_to_end() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000094, "Sua Submitter", &["auth.profile.read"])
        .await;
    let other_user = app
        .create_user(10000095, "Sua Other", &["auth.profile.read"])
        .await;

    let start = Utc::now() + Duration::days(1);
    let end = start + Duration::hours(1);

    // Bad flight-level format is rejected (must be exactly 3 digits).
    let bad_altitude_response = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": start.to_rfc3339(),
                "end_at": end.to_rfc3339(),
                "details": "Training sortie",
                "airspace": [{"identifier": "R-6608A", "bottom_altitude": "SFC", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&bad_altitude_response, StatusCode::BAD_REQUEST);

    // Too-short duration is rejected.
    let too_short_response = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": start.to_rfc3339(),
                "end_at": (start + Duration::minutes(10)).to_rfc3339(),
                "details": "Too short",
                "airspace": [{"identifier": "R-6608A", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&too_short_response, StatusCode::BAD_REQUEST);

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": start.to_rfc3339(),
                "end_at": end.to_rfc3339(),
                "details": "Training sortie",
                "airspace": [{"identifier": "R-6608A", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let created: Value = json_body(create_response).await;
    let mission_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["cid"], 10000094);
    assert!(created["mission_number"].as_str().unwrap().len() >= 4);
    let airspace = created["airspace"].as_array().unwrap();
    assert_eq!(airspace.len(), 1);
    assert_eq!(airspace[0]["identifier"], "R-6608A");

    // Self-listing shows only the submitter's own mission.
    let self_list_response = app
        .json_request(
            "GET",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&self_list_response, StatusCode::OK);
    let self_list: Value = json_body(self_list_response).await;
    assert_eq!(self_list["total"], 1);

    // A different user cannot delete someone else's mission.
    let denied_delete = app
        .json_request(
            "DELETE",
            &format!("/api/v1/sua/{mission_id}"),
            Some(&other_user.session_token),
            None,
        )
        .await;
    assert_status(&denied_delete, StatusCode::UNAUTHORIZED);

    // Public lookup by id works with no session at all.
    let lookup_by_id = app
        .json_request("GET", &format!("/api/v1/sua/{mission_id}"), None, None)
        .await;
    assert_status(&lookup_by_id, StatusCode::OK);
    let lookup_by_id_body: Value = json_body(lookup_by_id).await;
    assert_eq!(lookup_by_id_body["id"], mission_id);
    assert!(lookup_by_id_body.get("user_id").is_none());

    // Public lookup by mission_number also works, and matches the same mission.
    let mission_number = created["mission_number"].as_str().unwrap().to_string();
    let lookup_by_number = app
        .json_request("GET", &format!("/api/v1/sua/{mission_number}"), None, None)
        .await;
    assert_status(&lookup_by_number, StatusCode::OK);
    let lookup_by_number_body: Value = json_body(lookup_by_number).await;
    assert_eq!(lookup_by_number_body["id"], mission_id);

    // A lookup that matches nothing is a 404, not a 401/403.
    let lookup_missing = app
        .json_request("GET", "/api/v1/sua/does-not-exist", None, None)
        .await;
    assert_status(&lookup_missing, StatusCode::NOT_FOUND);

    // A third mission for the same user is rejected once the 2-active cap is hit.
    let second_create = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": (start + Duration::days(1)).to_rfc3339(),
                "end_at": (start + Duration::days(1) + Duration::hours(1)).to_rfc3339(),
                "details": "Second sortie",
                "airspace": [{"identifier": "R-6608B", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&second_create, StatusCode::CREATED);

    let third_create = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": (start + Duration::days(2)).to_rfc3339(),
                "end_at": (start + Duration::days(2) + Duration::hours(1)).to_rfc3339(),
                "details": "Third sortie",
                "airspace": [{"identifier": "R-6608C", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&third_create, StatusCode::BAD_REQUEST);

    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/sua/{mission_id}"),
            Some(&submitter.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn sua_upcoming_feed_is_public_windowed_and_expires_old_missions() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let submitter = app
        .create_user(10000096, "Sua Feed Submitter", &["auth.profile.read"])
        .await;

    // A mission starting soon (within the 2h window).
    let soon_start = Utc::now() + Duration::minutes(30);
    let soon_response = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": soon_start.to_rfc3339(),
                "end_at": (soon_start + Duration::hours(1)).to_rfc3339(),
                "details": "Soon sortie",
                "airspace": [{"identifier": "R-6608A", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&soon_response, StatusCode::CREATED);
    let soon_created: Value = json_body(soon_response).await;
    let soon_id = soon_created["id"].as_str().unwrap().to_string();

    // A mission starting well outside the window (next week) is excluded.
    let far_start = Utc::now() + Duration::days(7);
    let far_response = app
        .json_request(
            "POST",
            "/api/v1/sua/me",
            Some(&submitter.session_token),
            Some(json!({
                "afiliation": "CAP",
                "start_at": far_start.to_rfc3339(),
                "end_at": (far_start + Duration::hours(1)).to_rfc3339(),
                "details": "Far sortie",
                "airspace": [{"identifier": "R-6608B", "bottom_altitude": "000", "top_altitude": "180"}]
            })),
        )
        .await;
    assert_status(&far_response, StatusCode::CREATED);
    let far_created: Value = json_body(far_response).await;
    let far_id = far_created["id"].as_str().unwrap().to_string();

    // Directly seed an already-ended mission (2 hours past end) to verify expiration-on-read.
    let expired_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into org.sua_blocks (id, user_id, start_at, end_at, afiliation, details, mission_number, created_at, updated_at)
        values ($1, $2, now() - interval '4 hours', now() - interval '2 hours', 'CAP', 'Expired sortie', '9999', now(), now())
        "#,
    )
    .bind(&expired_id)
    .bind(&submitter.id)
    .execute(&app.pool)
    .await
    .expect("seed expired sua block");

    let feed_response = app.json_request("GET", "/api/v1/sua/upcoming", None, None).await;
    assert_status(&feed_response, StatusCode::OK);
    let feed: Value = json_body(feed_response).await;
    let items = feed["items"].as_array().unwrap();
    let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();

    assert!(ids.contains(&soon_id.as_str()), "soon mission should be in the feed");
    assert!(!ids.contains(&far_id.as_str()), "far mission should not be in the feed");
    assert!(!ids.contains(&expired_id.as_str()), "expired mission should not be in the feed");

    // The public item must not leak the internal user_id.
    let soon_item = items.iter().find(|i| i["id"] == soon_id).unwrap();
    assert!(soon_item.get("user_id").is_none());
    assert_eq!(soon_item["cid"], 10000096);

    // Expiration-on-read actually deleted the expired row.
    let still_there = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from org.sua_blocks where id = $1",
    )
    .bind(&expired_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(still_there, 0);
}
