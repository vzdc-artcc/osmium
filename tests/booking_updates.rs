//! `PUT /api/v1/bookings/{id}` against a fixture of the VATSIM booking API.

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use support::{EnvVarGuard, TestApp, assert_status, lock_env};

const OWNER_CID: i64 = 10000961;
const OTHER_CID: i64 = 10000962;
const BOOKING_ID: i64 = 555;

fn slot(hours_from_now: i64) -> String {
    (Utc::now() + Duration::hours(hours_from_now))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn booking(cid: i64, callsign: &str) -> Value {
    json!({
        "id": BOOKING_ID,
        "callsign": callsign,
        "cid": cid,
        "start": slot(3),
        "end": slot(4)
    })
}

/// Serves booking `BOOKING_ID` owned by `OWNER_CID` and counts upstream PUTs.
async fn serve_upstream(puts: Arc<AtomicUsize>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture upstream");
    let addr = listener.local_addr().expect("fixture address");
    let app = Router::new()
        .route(
            "/api/booking/{id}",
            get(|Path(_id): Path<i64>| async { Json(json!([booking(OWNER_CID, "DCA_GND")])) }).put(
                |State(puts): State<Arc<AtomicUsize>>, Json(body): Json<Value>| async move {
                    puts.fetch_add(1, Ordering::SeqCst);
                    let mut stored = body;
                    stored["id"] = json!(BOOKING_ID);
                    Json(stored)
                },
            ),
        )
        .with_state(puts);
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve fixture upstream");
    });
    format!("http://{addr}/api/booking")
}

#[tokio::test(flavor = "current_thread")]
async fn update_is_authorized_against_the_stored_booking_owner() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let puts = Arc::new(AtomicUsize::new(0));
    let _base = EnvVarGuard::set("ATC_BOOKING_BASE_URL", &serve_upstream(puts.clone()).await);
    let _token = EnvVarGuard::set("ATC_BOOKING_TOKEN", "test-token");

    let other = app
        .create_user(OTHER_CID, "Other Controller", &["auth.profile.update"])
        .await;
    let owner = app
        .create_user(OWNER_CID, "Booking Owner", &["auth.profile.update"])
        .await;

    // Someone else's booking, claimed by putting the caller's own cid in the body.
    let response = app
        .json_request(
            "PUT",
            &format!("/api/v1/bookings/{BOOKING_ID}"),
            Some(&other.session_token),
            Some(json!({ "callsign": "DCA_GND", "cid": OTHER_CID, "start": slot(5), "end": slot(6) })),
        )
        .await;
    // `ensure_permission` answers 401 on master and 403 once #99 lands;
    // either way the caller is refused.
    assert!(
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ),
        "expected a refusal, got {}",
        response.status()
    );
    assert_eq!(
        puts.load(Ordering::SeqCst),
        0,
        "the upstream write must not happen"
    );

    // The owner may update it, and the audit row carries both sides.
    let response = app
        .json_request(
            "PUT",
            &format!("/api/v1/bookings/{BOOKING_ID}"),
            Some(&owner.session_token),
            Some(json!({ "callsign": "DCA_TWR", "cid": OWNER_CID, "start": slot(5), "end": slot(6) })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(puts.load(Ordering::SeqCst), 1);

    let (action, before, after): (String, Value, Value) = sqlx::query_as(
        "select action, before_state, after_state from access.audit_logs where resource_type = 'ATC_BOOKING'",
    )
    .fetch_one(&app.pool)
    .await
    .expect("booking audit row");
    assert_eq!(action, "UPDATE");
    assert_eq!(before["callsign"], "DCA_GND");
    assert_eq!(after["callsign"], "DCA_TWR");

    app.cleanup().await;
}
