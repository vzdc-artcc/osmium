mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn certification_types_are_public_and_admin_route_stays_gated() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10000320, "Cert Type Staff", &["org.certifications.update"])
        .await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/admin/certification-types",
            Some(&staff.session_token),
            Some(json!({
                "name": "Ground Control",
                "can_solo_cert": true,
                "auto_assign_unrestricted": false,
                "certification_options": ["NONE", "GND", "CERTIFIED"]
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::OK);

    // No session at all: the public route succeeds where the admin one 401s.
    let public_response = app
        .json_request("GET", "/api/v1/certification-types", None, None)
        .await;
    assert_status(&public_response, StatusCode::OK);
    let public_body: Value = json_body(public_response).await;
    let items = public_body["items"].as_array().unwrap();
    assert!(
        items.iter().any(|item| item["name"] == "Ground Control"),
        "public route must surface the same catalog the admin route manages"
    );

    let admin_no_session = app
        .json_request("GET", "/api/v1/admin/certification-types", None, None)
        .await;
    assert_status(&admin_no_session, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn roster_certifications_are_public_and_admin_route_stays_gated() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000321,
            "Roster Cert Staff",
            &["org.certifications.update", "users.directory.read"],
        )
        .await;
    let controller = app
        .create_user(10000322, "Roster Cert Controller", &[])
        .await;

    let type_response = app
        .json_request(
            "POST",
            "/api/v1/admin/certification-types",
            Some(&staff.session_token),
            Some(json!({
                "name": "Approach",
                "can_solo_cert": false,
                "auto_assign_unrestricted": false,
                "certification_options": ["NONE", "APP", "CERTIFIED"]
            })),
        )
        .await;
    assert_status(&type_response, StatusCode::OK);
    let type_body: Value = json_body(type_response).await;
    let type_id = type_body["id"].as_str().unwrap().to_string();

    let save_response = app
        .json_request(
            "POST",
            &format!("/api/v1/users/{}/certifications", controller.cid),
            Some(&staff.session_token),
            Some(json!({
                "certifications": [{
                    "certification_type_id": type_id,
                    "certification_option": "CERTIFIED"
                }],
                "dossier_message": "granted for testing"
            })),
        )
        .await;
    assert_status(&save_response, StatusCode::OK);

    let public_response = app
        .json_request("GET", "/api/v1/roster-certifications", None, None)
        .await;
    assert_status(&public_response, StatusCode::OK);
    let public_body: Value = json_body(public_response).await;
    let items = public_body["items"].as_array().unwrap();
    let controller_row = items
        .iter()
        .find(|item| item["cid"] == controller.cid)
        .expect("public route must surface the controller's roster cert row");
    assert!(
        controller_row["certifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["certification_type_id"] == type_id),
        "public route must show the certification just granted"
    );
    assert!(
        controller_row.get("has_approved_loa").is_none(),
        "public roster item must not expose LOA status"
    );

    let admin_no_session = app
        .json_request("GET", "/api/v1/admin/roster-certifications", None, None)
        .await;
    assert_status(&admin_no_session, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn solo_certifications_are_public_through_a_narrower_dto() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000323,
            "Solo Cert Staff",
            &[
                "org.certifications.update",
                "users.controller_status.update",
            ],
        )
        .await;
    let controller = app.create_user(10000324, "Solo Controller", &[]).await;

    let type_response = app
        .json_request(
            "POST",
            "/api/v1/admin/certification-types",
            Some(&staff.session_token),
            Some(json!({
                "name": "Ground Solo",
                "can_solo_cert": true,
                "auto_assign_unrestricted": false,
                "certification_options": ["NONE", "GND", "SOLO"]
            })),
        )
        .await;
    assert_status(&type_response, StatusCode::OK);
    let type_body: Value = json_body(type_response).await;
    let type_id = type_body["id"].as_str().unwrap().to_string();

    let expires = Utc::now() + Duration::days(30);
    let create_solo_response = app
        .json_request(
            "POST",
            "/api/v1/admin/solo-certifications",
            Some(&staff.session_token),
            Some(json!({
                "user_id": controller.id,
                "certification_type_id": type_id,
                "position": "DCA_GND",
                "expires": expires.to_rfc3339()
            })),
        )
        .await;
    assert_status(&create_solo_response, StatusCode::CREATED);

    let public_response = app
        .json_request("GET", "/api/v1/solo-certifications", None, None)
        .await;
    assert_status(&public_response, StatusCode::OK);
    let public_body: Value = json_body(public_response).await;
    let items = public_body["items"].as_array().unwrap();
    let solo_row = items
        .iter()
        .find(|item| item["position"] == "DCA_GND")
        .expect("public route must surface the solo certification just granted");

    assert!(
        solo_row.get("id").is_none(),
        "public solo item must not expose the internal row id"
    );
    assert!(
        solo_row.get("user_id").is_none(),
        "public solo item must not expose the internal user_id"
    );
    assert!(
        solo_row.get("granted_by_actor_id").is_none(),
        "public solo item must not expose who granted it"
    );
    assert_eq!(solo_row["cid"], controller.cid);

    let admin_no_session = app
        .json_request("GET", "/api/v1/admin/solo-certifications", None, None)
        .await;
    assert_status(&admin_no_session, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}
