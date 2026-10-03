use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        AtcBookingItem, AtcBookingListResponse, CreateOrUpdateAtcBookingRequest,
        ListAtcBookingsQuery,
    },
    repos::{audit as audit_repo, stats as stats_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

const ATC_BOOKING_BASE: &str = "https://atc-bookings.vatsim.net/api/booking";

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

fn atc_token() -> Result<String, ApiError> {
    std::env::var("ATC_BOOKING_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .ok_or(ApiError::ServiceUnavailable)
}

fn atc_client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::Internal)
}

/// Whether a write to this booking requires the privileged
/// `training.appointments.update` gate rather than plain self-service. A
/// controller may only self-serve their own *non-training* bookings; editing
/// someone else's, or any `training` booking (the appointment-scheduler path),
/// is privileged.
fn booking_needs_privileged_auth(
    booking_cid: i64,
    caller_cid: i64,
    booking_type: Option<&str>,
) -> bool {
    booking_cid != caller_cid || booking_type == Some("training")
}

fn parse_booking_time(value: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ndt| ndt.and_utc())
}

/// Validation for non-training bookings, mirroring the website's
/// `createOrUpdateAtcBooking`: ≤2 active on create, made 2–72h in advance, and
/// ≤2h long. Returns the user-facing error string on failure.
fn validate_non_training_booking(
    start: &str,
    end: &str,
    now: DateTime<Utc>,
    active_count: usize,
    is_update: bool,
) -> Result<(), &'static str> {
    if !is_update && active_count >= 2 {
        return Err(
            "You have reached the maximum number of active bookings (2). Please delete an existing booking before creating a new one.",
        );
    }
    let start_dt = parse_booking_time(start).ok_or("Invalid start time.")?;
    let end_dt = parse_booking_time(end).ok_or("Invalid end time.")?;
    let advance = start_dt - now;
    if advance < Duration::hours(2) {
        return Err("Bookings must be made at least 2 hours in advance.");
    }
    if advance > Duration::hours(72) {
        return Err("Bookings can only be made up to 72 hours in advance.");
    }
    if end_dt - start_dt > Duration::hours(2) {
        return Err("Bookings can only be made for a maximum duration of 2 hours.");
    }
    Ok(())
}

fn booking_error_response(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "message": message.into() })),
    )
        .into_response()
}

async fn fetch_bookings(cid: Option<i64>) -> Result<Vec<AtcBookingItem>, ApiError> {
    let token = atc_token()?;
    let client = atc_client()?;
    let mut url = format!("{ATC_BOOKING_BASE}?key_only=1&sort=start");
    if let Some(cid) = cid {
        url.push_str(&format!("&cid={cid}"));
    }
    let res = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    if !res.status().is_success() {
        return Err(ApiError::ServiceUnavailable);
    }
    res.json::<Vec<AtcBookingItem>>()
        .await
        .map_err(|_| ApiError::Internal)
}

async fn fetch_single_booking(id: i64) -> Result<Option<AtcBookingItem>, ApiError> {
    let token = atc_token()?;
    let client = atc_client()?;
    let res = client
        .get(format!("{ATC_BOOKING_BASE}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let value: serde_json::Value = res.json().await.map_err(|_| ApiError::Internal)?;
    // The upstream returns an array for the by-id lookup.
    let candidate = match value {
        serde_json::Value::Array(mut arr) if !arr.is_empty() => arr.remove(0),
        serde_json::Value::Array(_) => return Ok(None),
        other => other,
    };
    Ok(serde_json::from_value(candidate).ok())
}

/// Result of one upstream create/update call.
enum UpstreamWrite {
    Ok(Box<AtcBookingItem>),
    NotFound,
    Error(String),
}

async fn send_booking_write(
    id: Option<i64>,
    body: &CreateOrUpdateAtcBookingRequest,
) -> Result<UpstreamWrite, ApiError> {
    let token = atc_token()?;
    let client = atc_client()?;
    let (url, method) = match id {
        Some(id) => (format!("{ATC_BOOKING_BASE}/{id}"), reqwest::Method::PUT),
        None => (ATC_BOOKING_BASE.to_string(), reqwest::Method::POST),
    };
    let mut payload = json!({
        "callsign": body.callsign,
        "cid": body.cid,
        "start": body.start,
        "end": body.end,
    });
    if let Some(t) = &body.r#type {
        payload["type"] = json!(t);
    }
    if let Some(d) = &body.division {
        payload["division"] = json!(d);
    }
    if let Some(s) = &body.subdivision {
        payload["subdivision"] = json!(s);
    }
    let res = client
        .request(method, &url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    let status = res.status();
    if status.is_success() {
        let item: AtcBookingItem = res.json().await.map_err(|_| ApiError::Internal)?;
        return Ok(UpstreamWrite::Ok(Box::new(item)));
    }
    if status == StatusCode::NOT_FOUND {
        return Ok(UpstreamWrite::NotFound);
    }
    let message = res
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "Error saving ATC booking.".to_string());
    Ok(UpstreamWrite::Error(message))
}

#[utoipa::path(get, path = "/api/v1/bookings", tag = "workflows", params(ListAtcBookingsQuery), responses((status = 200, description = "ATC bookings", body = AtcBookingListResponse), (status = 401, description = "Not authenticated"), (status = 503, description = "ATC booking service unavailable")))]
pub async fn list_atc_bookings(
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListAtcBookingsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<AtcBookingListResponse>, ApiError> {
    // ARTCC-wide calendar — any authenticated user.
    let _user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let items = fetch_bookings(query.cid).await?;
    Ok(ApiJson::new(AtcBookingListResponse { items }, time))
}

#[utoipa::path(get, path = "/api/v1/bookings/{id}", tag = "workflows", params(("id" = i64, Path, description = "Booking ID")), responses((status = 200, description = "ATC booking", body = AtcBookingItem), (status = 401, description = "Not authenticated"), (status = 404, description = "Booking not found"), (status = 503, description = "ATC booking service unavailable")))]
pub async fn get_atc_booking(
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(id): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<AtcBookingItem>, ApiError> {
    let _user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let item = fetch_single_booking(id).await?.ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(item, time))
}

async fn upsert_booking(
    state: &AppState,
    user: &CurrentUser,
    headers: &HeaderMap,
    id: Option<i64>,
    body: CreateOrUpdateAtcBookingRequest,
) -> Result<Response, ApiError> {
    // Data-dependent authorization.
    if booking_needs_privileged_auth(body.cid, user.cid, body.r#type.as_deref()) {
        ensure_permission(
            state,
            Some(user),
            None,
            PermissionPath::from_segments(["training", "appointments"], PermissionAction::Update),
        )
        .await?;
    } else {
        ensure_permission(
            state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
        )
        .await?;
    }

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let is_training = body.r#type.as_deref() == Some("training");

    // Advance/duration/limit checks apply to non-training bookings only.
    if !is_training {
        let active_count = if id.is_none() {
            fetch_bookings(Some(body.cid)).await?.len()
        } else {
            0
        };
        if let Err(message) = validate_non_training_booking(
            &body.start,
            &body.end,
            Utc::now(),
            active_count,
            id.is_some(),
        ) {
            return Ok(booking_error_response(message));
        }
    }

    // Callsign-prefix check — only when a booking `type` is set (matches the
    // website, which skips it for plain user bookings).
    if let Some(t) = body.r#type.as_deref() {
        if !t.is_empty() {
            if let Some(prefixes) = stats_repo::fetch_statistics_prefixes(pool).await? {
                if !prefixes
                    .prefixes
                    .iter()
                    .any(|prefix| body.callsign.starts_with(prefix))
                {
                    return Ok(booking_error_response(
                        "Error: Callsign is not valid for this ARTCC.",
                    ));
                }
            }
        }
    }

    // Audit context only: a failed lookup must not block the write.
    let before = match id {
        Some(id) => fetch_single_booking(id).await.ok().flatten(),
        None => None,
    };

    // Upstream write; a PUT against a missing booking falls back to create.
    let mut created = id.is_none();
    let mut outcome = send_booking_write(id, &body).await?;
    if matches!(outcome, UpstreamWrite::NotFound) && id.is_some() {
        outcome = send_booking_write(None, &body).await?;
        created = true;
    }

    match outcome {
        UpstreamWrite::Ok(item) => {
            record_booking_audit(
                pool,
                user,
                headers,
                if created { "CREATE" } else { "UPDATE" },
                Some(item.id.to_string()),
                if created { None } else { before.as_ref() },
                Some(&item),
            )
            .await?;
            Ok(Json(*item).into_response())
        }
        UpstreamWrite::NotFound => Ok(booking_error_response("ATC booking not found.")),
        UpstreamWrite::Error(message) => Ok(booking_error_response(message)),
    }
}

#[utoipa::path(post, path = "/api/v1/bookings", tag = "workflows", request_body = CreateOrUpdateAtcBookingRequest, responses((status = 200, description = "Created booking", body = AtcBookingItem), (status = 400, description = "Validation or upstream error", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Not authorized"), (status = 503, description = "ATC booking service unavailable")))]
pub async fn create_atc_booking(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(body): Json<CreateOrUpdateAtcBookingRequest>,
) -> Result<Response, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    upsert_booking(&state, user, &headers, None, body).await
}

#[utoipa::path(put, path = "/api/v1/bookings/{id}", tag = "workflows", params(("id" = i64, Path, description = "Booking ID")), request_body = CreateOrUpdateAtcBookingRequest, responses((status = 200, description = "Updated booking", body = AtcBookingItem), (status = 400, description = "Validation or upstream error", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Not authorized"), (status = 503, description = "ATC booking service unavailable")))]
pub async fn update_atc_booking(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<CreateOrUpdateAtcBookingRequest>,
) -> Result<Response, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    upsert_booking(&state, user, &headers, Some(id), body).await
}

#[utoipa::path(delete, path = "/api/v1/bookings/{id}", tag = "workflows", params(("id" = i64, Path, description = "Booking ID")), responses((status = 200, description = "Deleted booking", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Not authorized"), (status = 404, description = "Booking not found"), (status = 503, description = "ATC booking service unavailable")))]
pub async fn delete_atc_booking(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let booking = fetch_single_booking(id).await?.ok_or(ApiError::NotFound)?;

    if booking_needs_privileged_auth(booking.cid, user.cid, booking.r#type.as_deref()) {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["training", "appointments"], PermissionAction::Update),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
        )
        .await?;
    }

    let token = atc_token()?;
    let client = atc_client()?;
    let res = client
        .delete(format!("{ATC_BOOKING_BASE}/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    if !res.status().is_success() {
        return Err(ApiError::ServiceUnavailable);
    }

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    record_booking_audit(
        pool,
        user,
        &headers,
        "DELETE",
        Some(id.to_string()),
        Some(&booking),
        None,
    )
    .await?;

    Ok(Json(ApiMessageBody {
        message: "booking deleted".to_string(),
    }))
}

async fn record_booking_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_id: Option<String>,
    before: Option<&AtcBookingItem>,
    after: Option<&AtcBookingItem>,
) -> Result<(), ApiError> {
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: "ATC_BOOKING".to_string(),
            resource_id,
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            message: None,
            before_state: before.map(audit_repo::sanitized_snapshot).transpose()?,
            after_state: after.map(audit_repo::sanitized_snapshot).transpose()?,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{booking_needs_privileged_auth, validate_non_training_booking};
    use chrono::{Duration, Utc};

    fn ts(offset: Duration) -> String {
        (Utc::now() + offset)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    #[test]
    fn self_non_training_booking_is_unprivileged() {
        assert!(!booking_needs_privileged_auth(100, 100, None));
        assert!(!booking_needs_privileged_auth(100, 100, Some("booking")));
    }

    #[test]
    fn other_cid_or_training_is_privileged() {
        assert!(booking_needs_privileged_auth(200, 100, None));
        assert!(booking_needs_privileged_auth(100, 100, Some("training")));
    }

    #[test]
    fn valid_booking_passes() {
        let now = Utc::now();
        let start = ts(Duration::hours(3));
        let end = ts(Duration::hours(4));
        assert!(validate_non_training_booking(&start, &end, now, 0, false).is_ok());
    }

    #[test]
    fn third_active_booking_rejected_on_create_only() {
        let now = Utc::now();
        let start = ts(Duration::hours(3));
        let end = ts(Duration::hours(4));
        assert!(validate_non_training_booking(&start, &end, now, 2, false).is_err());
        // updates don't count against the limit
        assert!(validate_non_training_booking(&start, &end, now, 2, true).is_ok());
    }

    #[test]
    fn advance_window_enforced() {
        let now = Utc::now();
        let too_soon_start = ts(Duration::minutes(30));
        let too_soon_end = ts(Duration::hours(1));
        assert!(
            validate_non_training_booking(&too_soon_start, &too_soon_end, now, 0, false).is_err()
        );

        let too_far_start = ts(Duration::hours(80));
        let too_far_end = ts(Duration::hours(81));
        assert!(
            validate_non_training_booking(&too_far_start, &too_far_end, now, 0, false).is_err()
        );
    }

    #[test]
    fn max_duration_enforced() {
        let now = Utc::now();
        let start = ts(Duration::hours(3));
        let end = ts(Duration::hours(6)); // 3h long
        assert!(validate_non_training_booking(&start, &end, now, 0, false).is_err());
    }
}
