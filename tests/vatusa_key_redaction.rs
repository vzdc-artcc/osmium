//! Every VATUSA call sends the API key in the query string. These tests drive
//! each failing call through the real route, capture the log output, and
//! assert the key never appears in it.
//!
//! `VATUSA_API_BASE_URL` always points at a closed loopback port or a local
//! stub, so no request reaches the real VATUSA API.

mod support;

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde_json::{Value, json};
use support::{EnvVarGuard, TestApp, assert_status, json_body, lock_env};
use tracing_subscriber::fmt::MakeWriter;

const SENTINEL_KEY: &str = "integration-sentinel-key-177";
const UNREACHABLE_BASE_URL: &str = "http://127.0.0.1:1";

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log buffer lock")).into_owned()
    }

    /// Asserts nothing logged carries the key, then that each expected line
    /// was logged (proving the failure path ran). The leak check goes first so
    /// a regression fails on the leak, not on a changed error rendering.
    fn assert_logged_without_key(&self, expected: &[&str]) {
        let logs = self.text();
        assert!(
            !logs.contains(SENTINEL_KEY),
            "logs leak the API key:\n{logs}"
        );
        assert!(
            !logs.contains("apikey"),
            "logs carry the apikey query:\n{logs}"
        );
        for needle in expected {
            assert!(
                logs.contains(needle),
                "expected {needle:?} in logs:\n{logs}"
            );
        }
    }
}

impl Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("log buffer lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogs;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Routes this thread's `tracing` output into a buffer until the guard drops.
/// The tests run on a `current_thread` runtime, so the handler, the job code
/// and the log lines they emit all run on this thread.
fn capture_logs() -> (CapturedLogs, tracing::subscriber::DefaultGuard) {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    (logs, guard)
}

/// The env every case shares: a configured key and roster sync enabled, so the
/// code reaches the VATUSA call.
fn vatusa_env(base_url: &str) -> Vec<EnvVarGuard> {
    vec![
        EnvVarGuard::set("VATUSA_API_KEY", SENTINEL_KEY),
        EnvVarGuard::set("VATUSA_API_BASE_URL", base_url),
        EnvVarGuard::set("VATUSA_FACILITY_ID", "ZDC"),
        EnvVarGuard::set("ROSTER_SYNC_ENABLED", "true"),
    ]
}

#[derive(Clone, Copy)]
enum StubMode {
    /// Every endpoint answers 200 with a body that is not JSON.
    GarbageRoster,
    /// The rosters list `cid` as HOME; the user detail is not JSON.
    GarbageUserDetail { cid: i64 },
    /// Every roster request answers 500 with a plain-text body.
    ServerError,
}

#[derive(Clone)]
struct StubState {
    mode: StubMode,
    keys_seen: Arc<Mutex<Vec<String>>>,
}

/// A local VATUSA stand-in. Records the `apikey` each request carried, so a
/// test can prove the key was on the URL the error came from.
async fn serve_stub(mode: StubMode) -> (String, Arc<Mutex<Vec<String>>>) {
    let keys_seen = Arc::new(Mutex::new(Vec::new()));
    let state = StubState {
        mode,
        keys_seen: keys_seen.clone(),
    };

    async fn roster(
        State(state): State<StubState>,
        Query(query): Query<std::collections::HashMap<String, String>>,
        axum::extract::Path(membership): axum::extract::Path<String>,
    ) -> axum::response::Response {
        record_key(&state, &query);
        match state.mode {
            StubMode::GarbageRoster => "<html>maintenance</html>".into_response(),
            StubMode::ServerError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "upstream exploded").into_response()
            }
            StubMode::GarbageUserDetail { cid } => {
                let data = if membership == "home" {
                    json!([{ "cid": cid, "facility": "ZDC", "facility_join": null }])
                } else {
                    json!([])
                };
                axum::Json(json!({ "data": data })).into_response()
            }
        }
    }

    async fn user_detail(
        State(state): State<StubState>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> &'static str {
        record_key(&state, &query);
        "<html>maintenance</html>"
    }

    fn record_key(state: &StubState, query: &std::collections::HashMap<String, String>) {
        if let Some(key) = query.get("apikey") {
            state.keys_seen.lock().expect("keys lock").push(key.clone());
        }
    }

    let app = Router::new()
        .route("/facility/ZDC/roster/{membership}", get(roster))
        .route("/user/{cid}", get(user_detail))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind VATUSA stub");
    let addr = listener.local_addr().expect("stub address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve VATUSA stub") });
    (format!("http://{addr}"), keys_seen)
}

fn assert_stub_saw_key(keys_seen: &Arc<Mutex<Vec<String>>>) {
    let keys = keys_seen.lock().expect("keys lock");
    assert!(
        !keys.is_empty() && keys.iter().all(|key| key == SENTINEL_KEY),
        "stub should have received the key on every request, saw {keys:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn visitor_approval_send_failure_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let _vatusa_env = vatusa_env(UNREACHABLE_BASE_URL);
    let Some(app) = TestApp::new().await else {
        return;
    };

    let applicant = app
        .create_user(
            17700001,
            "Redaction Applicant",
            &["users.visitor_applications_self.request"],
        )
        .await;
    let staff = app
        .create_user(
            17700002,
            "Redaction Staff",
            &["users.visitor_applications.decide"],
        )
        .await;
    let created = app
        .json_request(
            "POST",
            "/api/v1/users/visitor-application",
            Some(&applicant.session_token),
            Some(json!({"home_facility": "ZLA", "why_visit": "Redaction test."})),
        )
        .await;
    assert_status(&created, StatusCode::OK);
    let created: Value = json_body(created).await;
    let application_id = created["id"].as_str().expect("application id");

    let (logs, _log_guard) = capture_logs();
    let response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/visitor-applications/{application_id}"),
            Some(&staff.session_token),
            Some(json!({"status": "APPROVED"})),
        )
        .await;
    assert_status(&response, StatusCode::SERVICE_UNAVAILABLE);

    logs.assert_logged_without_key(&[
        "vatusa manageVisitor request failed",
        "error sending request",
    ]);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn roster_removal_send_failure_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let _vatusa_env = vatusa_env(UNREACHABLE_BASE_URL);
    let Some(app) = TestApp::new().await else {
        return;
    };

    let atm = app
        .create_user(
            17700003,
            "Redaction ATM",
            &[
                "users.controller_status.update",
                "users.controller_status.delete",
            ],
        )
        .await;
    let target = app.create_user(17700004, "Redaction Target", &[]).await;
    let lifecycle_uri = format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid);

    let to_home = app
        .json_request(
            "PATCH",
            &lifecycle_uri,
            Some(&atm.session_token),
            Some(json!({"controller_status": "HOME"})),
        )
        .await;
    assert_status(&to_home, StatusCode::OK);

    let (logs, _log_guard) = capture_logs();
    let to_none = app
        .json_request(
            "PATCH",
            &lifecycle_uri,
            Some(&atm.session_token),
            Some(json!({"controller_status": "NONE"})),
        )
        .await;
    assert_status(&to_none, StatusCode::SERVICE_UNAVAILABLE);

    logs.assert_logged_without_key(&[
        "vatusa roster removal request failed",
        "error sending request",
    ]);

    app.cleanup().await;
}

/// Drives `POST /api/v1/admin/users/{cid}/refresh-vatusa`, which runs the
/// roster sync's fetch code for one user, and returns the captured logs.
async fn refresh_with_captured_logs(app: &TestApp, cid: i64) -> CapturedLogs {
    let staff = app
        .create_user(
            17700010,
            "Redaction Refresher",
            &["users.vatusa_refresh.request"],
        )
        .await;
    let (logs, _log_guard) = capture_logs();
    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{cid}/refresh-vatusa"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::SERVICE_UNAVAILABLE);
    logs
}

#[tokio::test(flavor = "current_thread")]
async fn roster_fetch_send_failure_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let _vatusa_env = vatusa_env(UNREACHABLE_BASE_URL);
    let Some(app) = TestApp::new().await else {
        return;
    };
    let target = app.create_user(17700011, "Redaction Member", &[]).await;

    let logs = refresh_with_captured_logs(&app, target.cid).await;

    logs.assert_logged_without_key(&["vatusa request failed", "error sending request"]);

    app.cleanup().await;
}

/// The non-success branch logs the URL built before the key is added, plus
/// the response body, so neither carries the key.
#[tokio::test(flavor = "current_thread")]
async fn roster_non_success_status_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let (base_url, keys_seen) = serve_stub(StubMode::ServerError).await;
    let _vatusa_env = vatusa_env(&base_url);
    let Some(app) = TestApp::new().await else {
        return;
    };
    let target = app.create_user(17700014, "Redaction Member", &[]).await;

    let logs = refresh_with_captured_logs(&app, target.cid).await;

    assert_stub_saw_key(&keys_seen);
    logs.assert_logged_without_key(&[
        "vatusa request returned non-success status",
        "status=500 Internal Server Error",
        "upstream exploded",
    ]);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn roster_decode_failure_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let (base_url, keys_seen) = serve_stub(StubMode::GarbageRoster).await;
    let _vatusa_env = vatusa_env(&base_url);
    let Some(app) = TestApp::new().await else {
        return;
    };
    let target = app.create_user(17700012, "Redaction Member", &[]).await;

    let logs = refresh_with_captured_logs(&app, target.cid).await;

    assert_stub_saw_key(&keys_seen);
    logs.assert_logged_without_key(&[
        "failed to parse vatusa roster response",
        "error decoding response body",
    ]);

    app.cleanup().await;
}

/// Covers the decode site in `fetch_user_detail` and the second log of the
/// same error in `map_user_detail_fetch_error`. The sync pass logs that same
/// `UserDetailFetchError::Decode` value too, but has no route to drive it.
#[tokio::test(flavor = "current_thread")]
async fn user_detail_decode_failure_does_not_log_the_api_key() {
    let _env_lock = lock_env();
    let cid = 17700013;
    let (base_url, keys_seen) = serve_stub(StubMode::GarbageUserDetail { cid }).await;
    let _vatusa_env = vatusa_env(&base_url);
    let Some(app) = TestApp::new().await else {
        return;
    };
    let target = app.create_user(cid, "Redaction Member", &[]).await;

    let logs = refresh_with_captured_logs(&app, target.cid).await;

    assert_stub_saw_key(&keys_seen);
    logs.assert_logged_without_key(&[
        "failed to parse vatusa user response",
        "failed to decode vatusa user response",
        "error decoding response body",
    ]);

    app.cleanup().await;
}
