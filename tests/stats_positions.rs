mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn controller_positions_lists_recorded_activations() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let controller = app.create_user(10000920, "Stats Controller", &[]).await;
    sqlx::raw_sql(&format!(
        "insert into stats.controller_sessions (id, environment, artcc_id, cid, login_at, logout_at, online_seconds, source_login_time_raw)
         values ('s-92', 'live', 'ZDC', {cid}, '2026-03-14 18:00:00+00', '2026-03-14 19:30:00+00', 5400, 'legacy:p-92');
         insert into stats.controller_activations (session_id, environment, cid, position_id, facility_name, position_name, position_type, default_callsign, is_primary, started_at, ended_at, active_seconds)
         values ('s-92', 'live', {cid}, 'legacy:p-92', 'DCA', 'DCA_GND', 'Ground', 'DCA_GND', true, '2026-03-14 18:00:00+00', '2026-03-14 19:30:00+00', 5400);",
        cid = controller.cid
    ))
    .execute(&app.pool)
    .await
    .expect("seed a closed session and activation");

    for query in ["?year=2026&month=3", ""] {
        let response = app
            .json_request(
                "GET",
                &format!(
                    "/api/v1/stats/controller/{}/positions{query}",
                    controller.cid
                ),
                None,
                None,
            )
            .await;
        assert_status(&response, StatusCode::OK);
        let body: Value = json_body(response).await;
        let items = body["items"].as_array().expect("items");
        assert_eq!(items.len(), 1, "query {query:?}: {body}");
        assert_eq!(items[0]["position_name"], "DCA_GND");
        assert_eq!(items[0]["active_seconds"], 5400);
    }

    let response = app
        .json_request(
            "GET",
            &format!(
                "/api/v1/stats/controller/{}/positions?year=2026&month=4",
                controller.cid
            ),
            None,
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["items"].as_array().map(Vec::len), Some(0));

    app.cleanup().await;
}
