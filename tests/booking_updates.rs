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
    response::IntoResponse,
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

/// How the fixture answers `GET /api/booking/{id}`.
#[derive(Clone, Copy)]
enum Lookup {
    /// Booking `BOOKING_ID`, owned by `OWNER_CID`.
    Owned,
    /// A bare status with no booking body.
    Status(StatusCode),
    /// `200` with a body that is not JSON.
    Garbage,
    /// `200` with valid JSON that is not a booking.
    NotABooking,
}

/// How the fixture behaves beyond the by-id lookup.
#[derive(Clone, Copy)]
struct Upstream {
    lookup: Lookup,
    /// How many active bookings `GET /api/booking` lists for `OWNER_CID`.
    active: usize,
    /// `PUT /api/booking/{id}` answers `404`, as when the booking is deleted
    /// between the lookup and the write.
    put_gone: bool,
}

impl Upstream {
    fn new(lookup: Lookup) -> Self {
        Self {
            lookup,
            active: 0,
            put_gone: false,
        }
    }

    fn active(self, active: usize) -> Self {
        Self { active, ..self }
    }

    fn put_gone(self) -> Self {
        Self {
            put_gone: true,
            ..self
        }
    }
}

#[derive(Clone, Default)]
struct Writes {
    puts: Arc<AtomicUsize>,
    posts: Arc<AtomicUsize>,
}

fn stored(mut body: Value) -> Json<Value> {
    body["id"] = json!(BOOKING_ID);
    Json(body)
}

/// A fixture of the VATSIM booking API that counts upstream PUTs and POSTs.
async fn serve_upstream(upstream: Upstream, writes: Writes) -> String {
    let Upstream {
        lookup,
        active,
        put_gone,
    } = upstream;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture upstream");
    let addr = listener.local_addr().expect("fixture address");
    let app = Router::new()
        .route(
            "/api/booking",
            get(move || async move {
                let items: Vec<Value> = (0..active)
                    .map(|n| {
                        let mut item = booking(OWNER_CID, "DCA_GND");
                        item["id"] = json!(1000 + n as i64);
                        item
                    })
                    .collect();
                Json(Value::Array(items))
            })
            .post(
                |State(w): State<Writes>, Json(body): Json<Value>| async move {
                    w.posts.fetch_add(1, Ordering::SeqCst);
                    stored(body)
                },
            ),
        )
        .route(
            "/api/booking/{id}",
            get(move |Path(_id): Path<i64>| async move {
                match lookup {
                    Lookup::Owned => Json(json!([booking(OWNER_CID, "DCA_GND")])).into_response(),
                    Lookup::Status(status) => status.into_response(),
                    Lookup::Garbage => "<html>maintenance</html>".into_response(),
                    Lookup::NotABooking => {
                        Json(json!([{ "message": "Too Many Requests" }])).into_response()
                    }
                }
            })
            .put(
                move |State(w): State<Writes>, Json(body): Json<Value>| async move {
                    w.puts.fetch_add(1, Ordering::SeqCst);
                    if put_gone {
                        StatusCode::NOT_FOUND.into_response()
                    } else {
                        stored(body).into_response()
                    }
                },
            ),
        )
        .with_state(writes);
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve fixture upstream");
    });
    format!("http://{addr}/api/booking")
}

async fn booking_audit_rows(app: &TestApp) -> Vec<(String, Option<Value>, Option<Value>)> {
    sqlx::query_as(
        "select action, before_state, after_state from access.audit_logs where resource_type = 'ATC_BOOKING' order by created_at",
    )
    .fetch_all(&app.pool)
    .await
    .expect("booking audit rows")
}

#[tokio::test(flavor = "current_thread")]
async fn update_is_authorized_against_the_stored_booking_owner() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let writes = Writes::default();
    let puts = writes.puts.clone();
    let _base = EnvVarGuard::set(
        "ATC_BOOKING_BASE_URL",
        &serve_upstream(Upstream::new(Lookup::Owned).active(2), writes).await,
    );
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

    // The owner may update it although they hold two active bookings (an
    // update is not counted), and the audit row carries both sides.
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

/// A failed lookup cannot be told apart from a booking someone else owns, so
/// it refuses the update instead of skipping the owner check.
#[tokio::test(flavor = "current_thread")]
async fn update_is_refused_when_the_stored_booking_cannot_be_read() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let _token = EnvVarGuard::set("ATC_BOOKING_TOKEN", "test-token");
    let other = app
        .create_user(OTHER_CID, "Other Controller", &["auth.profile.update"])
        .await;

    let cases = [
        (
            "429",
            Lookup::Status(StatusCode::TOO_MANY_REQUESTS),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            "500",
            Lookup::Status(StatusCode::INTERNAL_SERVER_ERROR),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            "non-JSON body",
            Lookup::Garbage,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "JSON that is not a booking",
            Lookup::NotABooking,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ];
    for (name, lookup, expected) in cases {
        let writes = Writes::default();
        let _base = EnvVarGuard::set(
            "ATC_BOOKING_BASE_URL",
            &serve_upstream(Upstream::new(lookup), writes.clone()).await,
        );
        let response = app
            .json_request(
                "PUT",
                &format!("/api/v1/bookings/{BOOKING_ID}"),
                Some(&other.session_token),
                Some(json!({ "callsign": "DCA_GND", "cid": OTHER_CID, "start": slot(5), "end": slot(6) })),
            )
            .await;
        assert_eq!(response.status(), expected, "lookup answering {name}");
        assert_eq!(
            writes.puts.load(Ordering::SeqCst),
            0,
            "no PUT after a {name} lookup"
        );
        assert_eq!(
            writes.posts.load(Ordering::SeqCst),
            0,
            "no POST after a {name} lookup"
        );
    }
    assert!(booking_audit_rows(&app).await.is_empty());

    app.cleanup().await;
}

/// A genuine `404` falls back to create: the upstream gets a POST, never a
/// PUT, and the audit row says `CREATE` with no before-state.
#[tokio::test(flavor = "current_thread")]
async fn update_of_a_missing_booking_creates_it_and_is_audited_as_create() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let _token = EnvVarGuard::set("ATC_BOOKING_TOKEN", "test-token");
    let writes = Writes::default();
    let _base = EnvVarGuard::set(
        "ATC_BOOKING_BASE_URL",
        &serve_upstream(
            Upstream::new(Lookup::Status(StatusCode::NOT_FOUND)).active(1),
            writes.clone(),
        )
        .await,
    );
    let owner = app
        .create_user(OWNER_CID, "Booking Owner", &["auth.profile.update"])
        .await;

    let response = app
        .json_request(
            "PUT",
            &format!("/api/v1/bookings/{BOOKING_ID}"),
            Some(&owner.session_token),
            Some(json!({ "callsign": "DCA_TWR", "cid": OWNER_CID, "start": slot(5), "end": slot(6) })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(writes.puts.load(Ordering::SeqCst), 0);
    assert_eq!(writes.posts.load(Ordering::SeqCst), 1);

    let rows = booking_audit_rows(&app).await;
    assert_eq!(rows.len(), 1);
    let (action, before, after) = &rows[0];
    assert_eq!(action, "CREATE");
    assert!(before.is_none(), "a create has no before-state: {before:?}");
    assert_eq!(after.as_ref().expect("after-state")["callsign"], "DCA_TWR");

    app.cleanup().await;
}

const LIMIT_MESSAGE: &str = "You have reached the maximum number of active bookings (2). Please delete an existing booking before creating a new one.";

/// A PUT that ends in a create is held to the two-active limit, whichever way
/// it reaches the create: a lookup that finds no booking, or a booking that
/// disappears between the lookup and the upstream PUT.
#[tokio::test(flavor = "current_thread")]
async fn update_that_creates_is_held_to_the_active_booking_limit() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let _token = EnvVarGuard::set("ATC_BOOKING_TOKEN", "test-token");
    let owner = app
        .create_user(OWNER_CID, "Booking Owner", &["auth.profile.update"])
        .await;

    let cases = [
        (
            "lookup 404",
            Upstream::new(Lookup::Status(StatusCode::NOT_FOUND)),
            0,
        ),
        (
            "PUT 404 after the lookup found it",
            Upstream::new(Lookup::Owned).put_gone(),
            1,
        ),
    ];
    for (name, upstream, expected_puts) in cases {
        let writes = Writes::default();
        let _base = EnvVarGuard::set(
            "ATC_BOOKING_BASE_URL",
            &serve_upstream(upstream.active(2), writes.clone()).await,
        );
        let response = app
            .json_request(
                "PUT",
                &format!("/api/v1/bookings/{BOOKING_ID}"),
                Some(&owner.session_token),
                Some(json!({ "callsign": "DCA_TWR", "cid": OWNER_CID, "start": slot(5), "end": slot(6) })),
            )
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name}");
        let body: Value = support::json_body(response).await;
        assert_eq!(body["message"], LIMIT_MESSAGE, "{name}");
        assert_eq!(
            writes.posts.load(Ordering::SeqCst),
            0,
            "no create after a {name}"
        );
        assert_eq!(writes.puts.load(Ordering::SeqCst), expected_puts, "{name}");

        // Positive control: one active booking leaves room for the create.
        let writes = Writes::default();
        let _base = EnvVarGuard::set(
            "ATC_BOOKING_BASE_URL",
            &serve_upstream(upstream.active(1), writes.clone()).await,
        );
        let response = app
            .json_request(
                "PUT",
                &format!("/api/v1/bookings/{BOOKING_ID}"),
                Some(&owner.session_token),
                Some(json!({ "callsign": "DCA_TWR", "cid": OWNER_CID, "start": slot(5), "end": slot(6) })),
            )
            .await;
        assert_status(&response, StatusCode::OK);
        assert_eq!(
            writes.posts.load(Ordering::SeqCst),
            1,
            "create after a {name}"
        );
    }

    let actions: Vec<String> = booking_audit_rows(&app)
        .await
        .into_iter()
        .map(|(action, _, _)| action)
        .collect();
    assert_eq!(
        actions,
        ["CREATE", "CREATE"],
        "only the two controls are audited"
    );

    app.cleanup().await;
}
