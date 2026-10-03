//! What the stats sync counts: positions vNAS assigns to ZDC on the live
//! network, never ATIS or another ARTCC's positions, with tower-cab (`Atct`)
//! time in the right delivery/ground/tower bucket and each connection
//! credited once, to its primary position.

mod support;

use std::sync::{Arc, Mutex};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use osmium::jobs::stats_sync::{StatsEnvironment, run_once};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use support::{EnvVarGuard, TestApp, assert_status, lock_env};

const ZDC_CID: i64 = 10000941;
const ZNY_CID: i64 = 10000942;
const ATIS_CID: i64 = 10000943;

fn controller(artcc: &str, cid: i64, callsign: &str, positions: Value) -> Value {
    json!({
        "artccId": artcc,
        "primaryFacilityId": "DCA",
        "primaryPositionId": "pos-1",
        "role": "Controller",
        "isActive": true,
        "isObserver": false,
        // Relative to the real clock: the sync closes sessions at poll time.
        "loginTime": (chrono::Utc::now() - chrono::Duration::seconds(30)).to_rfc3339(),
        "vatsimData": {
            "cid": cid.to_string(),
            "realName": "Feed Controller",
            "userRating": "S2",
            "requestedRating": "S2",
            "callsign": callsign
        },
        "positions": positions
    })
}

fn cab_position(id: &str, default_callsign: &str, is_primary: bool) -> Value {
    json!({
        "facilityId": "DCA",
        "facilityName": "Reagan National",
        "positionId": id,
        "positionName": default_callsign,
        "positionType": "Atct",
        "radioName": "National",
        "defaultCallsign": default_callsign,
        "frequency": 121700000,
        "isPrimary": is_primary,
        "isActive": true
    })
}

/// Serves whatever feed the test currently holds, so successive polls can see
/// controllers log on and off.
async fn serve_feed(feed: Arc<Mutex<Value>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture feed");
    let addr = listener.local_addr().expect("fixture address");
    let app = Router::new()
        .route(
            "/controllers.json",
            get(|State(feed): State<Arc<Mutex<Value>>>| async move {
                Json(feed.lock().unwrap().clone())
            }),
        )
        .with_state(feed);
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve fixture feed");
    });
    format!("http://{addr}/controllers.json")
}

#[tokio::test(flavor = "current_thread")]
async fn sync_counts_zdc_positions_only_and_credits_the_primary_cab_position() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let feed = Arc::new(Mutex::new(json!({
        "updatedAt": "2026-10-03T20:00:00Z",
        "controllers": [
            // Ground, also covering delivery as a consolidated secondary.
            controller("ZDC", ZDC_CID, "DCA_GND", json!([
                cab_position("pos-dca-gnd", "DCA_GND", true),
                cab_position("pos-dca-del", "DCA_DEL", false),
            ])),
            // A ZDC home controller working New York: vNAS reports ZNY.
            controller("ZNY", ZNY_CID, "JFK_GND", json!([cab_position("pos-jfk-gnd", "JFK_GND", true)])),
            controller("ZDC", ATIS_CID, "DCA_ATIS", json!([])),
        ]
    })));
    let _url = EnvVarGuard::set(
        "VNAS_CONTROLLER_FEED_URL_LIVE",
        &serve_feed(feed.clone()).await,
    );

    run_once(app.state.clone(), StatsEnvironment::Live)
        .await
        .expect("first poll");

    let sessions: Vec<i64> =
        sqlx::query_scalar("select cid from stats.controller_sessions order by cid")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        sessions,
        vec![ZDC_CID],
        "only the ZDC, non-ATIS connection counts"
    );

    let position_types: Vec<String> = sqlx::query_scalar(
        "select position_type from stats.controller_activations where cid = $1 order by position_type",
    )
    .bind(ZDC_CID)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert_eq!(position_types, vec!["Delivery", "Ground"]);

    // The controller logs off; closing the activations credits the rollup.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    *feed.lock().unwrap() = json!({ "updatedAt": "2026-10-03T20:00:05Z", "controllers": [] });
    run_once(app.state.clone(), StatsEnvironment::Live)
        .await
        .expect("second poll");

    // Summed: the session may straddle a month boundary.
    let (ground, delivery, online): (i64, i64, i64) = sqlx::query_as(
        "select coalesce(sum(ground_seconds), 0)::bigint, coalesce(sum(delivery_seconds), 0)::bigint, coalesce(sum(online_seconds), 0)::bigint
         from stats.controller_monthly_rollups where environment = 'live' and cid = $1",
    )
    .bind(ZDC_CID)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        ground >= 1,
        "primary cab time credited to ground, got {ground}"
    );
    assert_eq!(delivery, 0, "a secondary position adds no hours");
    assert!(online >= ground);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn stats_endpoints_refuse_sweatbox_environments() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let controller = app.create_user(10000944, "Stats Controller", &[]).await;

    for path in [
        format!(
            "/api/v1/stats/controller/{}/positions?environment=sweatbox1",
            controller.cid
        ),
        format!(
            "/api/v1/stats/controller/{}/totals?environment=sweatbox2",
            controller.cid
        ),
        "/api/v1/stats/artcc?environment=sweatbox1".to_string(),
    ] {
        let response = app.json_request("GET", &path, None, None).await;
        assert_status(&response, StatusCode::BAD_REQUEST);
    }

    app.cleanup().await;
}

/// The live-only migration deletes sweatbox stats and credits primary cab
/// time that was stored as `Atct`.
#[tokio::test(flavor = "current_thread")]
async fn live_only_migration_repairs_cab_time_and_drops_sweatbox_rows() {
    let _env_lock = lock_env();
    let Ok(root_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let database_name = format!("osmium_test_{}", uuid::Uuid::new_v4().simple());
    let mut database_url = reqwest::Url::parse(&root_url).expect("parse DATABASE_URL");
    database_url.set_path(&format!("/{database_name}"));
    let root = PgPoolOptions::new()
        .max_connections(1)
        .connect(&root_url)
        .await
        .expect("connect root database");
    sqlx::query(&format!("create database \"{database_name}\""))
        .execute(&root)
        .await
        .expect("create database");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url.as_str())
        .await
        .expect("connect test database");

    let migrator = sqlx::migrate!("./migrations");
    let repair_version = migrator
        .iter()
        .find(|migration| migration.description == "stats live only and atct repair")
        .expect("repair migration exists")
        .version;
    let apply = |version: i64| {
        let sql = migrator
            .iter()
            .find(|migration| migration.version == version)
            .expect("migration exists")
            .sql
            .to_string();
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.expect("begin");
            // A non-UTC session must not move month boundaries.
            sqlx::raw_sql("set local time zone 'America/New_York'")
                .execute(&mut *tx)
                .await
                .expect("set session time zone");
            sqlx::raw_sql(&sql)
                .execute(&mut *tx)
                .await
                .expect("apply migration");
            tx.commit().await.expect("commit");
        }
    };
    for version in migrator
        .iter()
        .map(|m| m.version)
        .filter(|v| *v < repair_version)
    {
        apply(version).await;
    }

    // A primary Atct ground activation spanning the March/April boundary
    // (7200s in March, 3600s in April), against a March rollup that already
    // holds online time, plus a secondary Atct delivery activation that must
    // not be credited.
    sqlx::raw_sql(
        "insert into stats.controller_sessions (id, environment, artcc_id, cid, login_at, logout_at, online_seconds, source_login_time_raw)
         values ('s-live', 'live', 'ZDC', 7600001, '2026-03-31 22:00:00+00', '2026-04-01 01:00:00+00', 10800, 'x'),
                ('s-sb', 'sweatbox1', 'ZDC', 7600001, '2026-03-31 22:00:00+00', '2026-04-01 01:00:00+00', 10800, 'x');
         insert into stats.controller_activations (id, session_id, environment, cid, position_id, facility_name, position_name, position_type, default_callsign, is_primary, started_at, ended_at, active_seconds)
         values ('a-live', 's-live', 'live', 7600001, 'p', 'Reagan National', 'Ground', 'Atct', 'DCA_GND', true, '2026-03-31 22:00:00+00', '2026-04-01 01:00:00+00', 10800),
                ('a-live-del', 's-live', 'live', 7600001, 'd', 'Reagan National', 'Delivery', 'Atct', 'DCA_DEL', false, '2026-03-31 22:00:00+00', '2026-04-01 01:00:00+00', 10800),
                ('a-sb', 's-sb', 'sweatbox1', 7600001, 'p', 'Reagan National', 'Ground', 'Atct', 'DCA_GND', true, '2026-03-31 22:00:00+00', '2026-04-01 01:00:00+00', 10800);
         insert into stats.controller_monthly_rollups (environment, cid, year, month, online_seconds)
         values ('live', 7600001, 2026, 2, 7200), ('sweatbox1', 7600001, 2026, 2, 7200);
         insert into stats.controller_feed_state (environment, endpoint_url) values ('live', 'x'), ('sweatbox2', 'x');
         insert into stats.controller_events (environment, event_type, cid, occurred_at, payload)
         values ('live', 'session_started', 7600001, now(), '{}'), ('sweatbox1', 'session_started', 7600001, now(), '{}');",
    )
    .execute(&pool)
    .await
    .expect("seed stats rows");

    apply(repair_version).await;

    let types: Vec<String> =
        sqlx::query_scalar("select position_type from stats.controller_activations order by id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(types, vec!["Ground", "Delivery"]);

    let rollups: Vec<(i32, i64, i64, i64)> = sqlx::query_as(
        "select month, online_seconds, ground_seconds, delivery_seconds from stats.controller_monthly_rollups where cid = 7600001 order by environment, month",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rollups,
        vec![(2, 7200, 7200, 0), (3, 0, 3600, 0)],
        "live only, primary only, split at the UTC month boundary"
    );

    for table in [
        "controller_sessions",
        "controller_feed_state",
        "controller_events",
    ] {
        let non_live: i64 = sqlx::query_scalar(&format!(
            "select count(*) from stats.{table} where environment <> 'live'"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(non_live, 0, "{table} still has sweatbox rows");
        let live: i64 = sqlx::query_scalar(&format!(
            "select count(*) from stats.{table} where environment = 'live'"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live, 1, "{table} live row must survive");
    }
    let sweatbox_activations: i64 = sqlx::query_scalar(
        "select count(*) from stats.controller_activations where environment <> 'live'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sweatbox_activations, 0);

    pool.close().await;
    sqlx::query(&format!(
        "drop database if exists \"{database_name}\" with (force)"
    ))
    .execute(&root)
    .await
    .expect("drop test database");
}
