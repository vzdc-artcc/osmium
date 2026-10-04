mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn user_event_positions_are_paged_newest_first() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let controller = app
        .create_user(10000164, "Event Regular", &["auth.profile.read"])
        .await;

    // Three published positions on three events, plus one unpublished.
    sqlx::raw_sql(&format!(
        "insert into events.events (id, title, starts_at, ends_at, created_by) values
             ('e1', 'January FNO', '2026-01-10 23:00+00', '2026-01-11 03:00+00', '{uid}'),
             ('e2', 'February FNO', '2026-02-14 23:00+00', '2026-02-15 03:00+00', '{uid}'),
             ('e3', 'March FNO', '2026-03-14 23:00+00', '2026-03-15 03:00+00', '{uid}'),
             ('e4', 'April FNO', '2026-04-11 23:00+00', '2026-04-12 03:00+00', '{uid}');
         insert into events.event_positions (event_id, callsign, user_id, published) values
             ('e1', 'DCA_GND', '{uid}', true),
             ('e2', 'DCA_TWR', '{uid}', true),
             ('e3', 'PCT_APP', '{uid}', true),
             ('e4', 'DC_CTR', '{uid}', false);",
        uid = controller.id
    ))
    .execute(&app.pool)
    .await
    .expect("seed events and positions");

    let url = |n: i64| {
        format!(
            "/api/v1/users/{}/event-positions?page={n}&page_size=2",
            controller.cid
        )
    };

    let response = app
        .json_request("GET", &url(1), Some(&controller.session_token), None)
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let titles: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["event_title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["March FNO", "February FNO"]);
    assert_eq!(body["total"], 3, "unpublished positions are not counted");
    assert_eq!(body["has_next"], true);

    let response = app
        .json_request("GET", &url(2), Some(&controller.session_token), None)
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["event_title"], "January FNO");
    assert_eq!(body["has_next"], false);

    app.cleanup().await;
}
