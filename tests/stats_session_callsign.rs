mod support;

use axum::{Json, Router, http::StatusCode, routing::get};
use osmium::jobs::stats_sync::{StatsEnvironment, run_once};
use serde_json::{Value, json};
use support::{EnvVarGuard, TestApp, assert_status, json_body, lock_env};

const CID: i64 = 10000930;

/// One ZDC controller logged in as `DCA_GND`, staffing a position whose
/// configured default callsign is deliberately different, so the test can only
/// pass if the logged-in callsign is the one stored.
fn fixture_feed() -> Value {
    json!({
        "updatedAt": "2026-10-03T20:00:00Z",
        "controllers": [{
            "artccId": "ZDC",
            "primaryFacilityId": "DCA",
            "primaryPositionId": "pos-dca-gnd-w",
            "role": "Controller",
            "isActive": true,
            "isObserver": false,
            "loginTime": "2026-10-03T19:30:00Z",
            "vatsimData": {
                "cid": CID.to_string(),
                "realName": "Callsign Controller",
                "userRating": "S2",
                "requestedRating": "S2",
                "callsign": "DCA_GND"
            },
            "positions": [{
                "facilityId": "DCA",
                "facilityName": "Reagan National",
                "positionId": "pos-dca-gnd-w",
                "positionName": "Ground West",
                "positionType": "Ground",
                "radioName": "National Ground",
                "defaultCallsign": "DCA_W_GND",
                "frequency": 121700000,
                "isPrimary": true,
                "isActive": true
            }, {
                "facilityId": "DCA",
                "facilityName": "Reagan National",
                "positionId": "pos-dca-del",
                "positionName": "Clearance Delivery",
                "positionType": "Delivery",
                "radioName": "National Clearance",
                "defaultCallsign": "DCA_DEL",
                "frequency": 128250000,
                "isPrimary": false,
                "isActive": true
            }]
        }]
    })
}

async fn serve_fixture() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture feed");
    let addr = listener.local_addr().expect("fixture address");
    let feed = Router::new().route("/controllers.json", get(|| async { Json(fixture_feed()) }));
    tokio::spawn(async move {
        axum::serve(listener, feed)
            .await
            .expect("serve fixture feed");
    });
    format!("http://{addr}/controllers.json")
}

async fn listed_callsigns(app: &TestApp) -> Vec<Value> {
    let response = app
        .json_request(
            "GET",
            &format!("/api/v1/stats/controller/{CID}/positions"),
            None,
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    body["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["callsign"].clone())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn stats_sync_stores_and_exposes_the_logged_in_callsign() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let _feed = EnvVarGuard::set("VNAS_CONTROLLER_FEED_URL_LIVE", &serve_fixture().await);

    run_once(app.state.clone(), StatsEnvironment::Live)
        .await
        .expect("run one live stats sync");

    let stored: Option<String> = sqlx::query_scalar(
        "select callsign from stats.controller_sessions where environment = 'live' and cid = $1",
    )
    .bind(CID)
    .fetch_one(&app.pool)
    .await
    .expect("session written by the sync");
    assert_eq!(stored.as_deref(), Some("DCA_GND"));

    // The logged-in callsign names the primary position; the consolidated
    // secondary position keeps its own default callsign.
    let mut listed = listed_callsigns(&app).await;
    listed.sort_by_key(|value| value.to_string());
    assert_eq!(listed, vec![json!("DCA_DEL"), json!("DCA_GND")]);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn positions_fall_back_to_the_default_callsign_for_older_sessions() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    sqlx::raw_sql(&format!(
        "insert into stats.controller_sessions (id, environment, artcc_id, cid, login_at, logout_at, online_seconds, source_login_time_raw)
         values ('s-93', 'live', 'ZDC', {CID}, '2026-03-14 18:00:00+00', '2026-03-14 19:00:00+00', 3600, '2026-03-14T18:00:00Z');
         insert into stats.controller_activations (session_id, environment, cid, position_id, facility_name, position_name, position_type, default_callsign, is_primary, started_at, ended_at, active_seconds)
         values ('s-93', 'live', {CID}, 'pos-1', 'Reagan National', 'Ground West', 'Ground', 'DCA_W_GND', true, '2026-03-14 18:00:00+00', '2026-03-14 19:00:00+00', 3600);"
    ))
    .execute(&app.pool)
    .await
    .expect("seed a session recorded before callsigns were stored");

    assert_eq!(listed_callsigns(&app).await, vec![json!("DCA_W_GND")]);

    app.cleanup().await;
}
