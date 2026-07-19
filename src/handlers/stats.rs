use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{Datelike, Utc};

use crate::auth::context::{CurrentServiceAccount, CurrentUser};
use crate::auth::permissions::{StatsPrefixesRead, StatsPrefixesUpdate};
use crate::auth::require_permission::RequirePermission;
use crate::models::stats::{
    ArtccStatsQuery, ArtccStatsResponse, ControllerEventItem, ControllerEventsQuery,
    ControllerEventsResponse, ControllerHistoryQuery, ControllerHistoryResponse, ControllerLeader,
    ControllerTotals, ControllerTotalsResponse, MonthlyBucket, StatisticsPrefixes,
    UpdateStatisticsPrefixesRequest,
};
use crate::repos::audit as audit_repo;
use crate::repos::stats as stats_repo;
use crate::repos::stats::{ControllerIdentityRow, ControllerTotalsRow};
use crate::time::{ApiJson, ResponseTimeContext};
use crate::{errors::ApiError, jobs::stats_sync::parse_environment, state::AppState};

#[utoipa::path(
    get,
    path = "/api/v1/stats/artcc",
    tag = "stats",
    params(
        ("environment" = Option<String>, Query, description = "Environment: live, sweatbox1, or sweatbox2"),
        ("all_time" = Option<bool>, Query, description = "Return all-time totals"),
        ("month" = Option<i32>, Query, description = "Month 1-12"),
        ("year" = Option<i32>, Query, description = "Calendar year"),
        ("top" = Option<i64>, Query, description = "Leader count"),
        ("limit" = Option<i64>, Query, description = "Controller row count")
    ),
    responses(
        (status = 200, description = "ARTCC statistics summary", body = ArtccStatsResponse),
        (status = 400, description = "Invalid query")
    )
)]
pub async fn get_artcc_stats(
    State(state): State<AppState>,
    Query(query): Query<ArtccStatsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ArtccStatsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let environment = parse_environment(query.environment.as_deref())?;
    let environment_name = environment.as_str().to_string();

    let now = Utc::now();
    let all_time = query.all_time.unwrap_or(false);
    let selected_year = query.year.unwrap_or(now.year());
    if !all_time && !(2000..=2100).contains(&selected_year) {
        return Err(ApiError::BadRequest);
    }

    let month_input = if all_time { None } else { query.month };
    let month_zero_based = match month_input {
        Some(month) if (1..=12).contains(&month) => Some(month - 1),
        Some(_) => return Err(ApiError::BadRequest),
        None => None,
    };

    let top = query.top.unwrap_or(3).clamp(1, 25) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let query_limit = std::cmp::max(limit, top as i64);

    let updated_at = stats_repo::last_feed_updated_at(pool, environment.as_str()).await?;

    let controller_count = stats_repo::count_controllers(
        pool,
        environment.as_str(),
        all_time,
        selected_year,
        month_zero_based,
    )
    .await?;

    let summary = stats_repo::fetch_artcc_summary(
        pool,
        environment.as_str(),
        all_time,
        selected_year,
        month_zero_based,
    )
    .await?;

    let controller_rows = stats_repo::list_controller_totals_rows(
        pool,
        environment.as_str(),
        all_time,
        selected_year,
        month_zero_based,
        query_limit,
    )
    .await?;

    let all_controllers = controller_rows
        .iter()
        .map(row_to_controller_totals)
        .collect::<Vec<_>>();

    let controllers = all_controllers
        .iter()
        .take(limit as usize)
        .cloned()
        .collect::<Vec<_>>();

    let leaders = all_controllers
        .iter()
        .take(top)
        .enumerate()
        .map(|(idx, row)| ControllerLeader {
            rank: (idx + 1) as i32,
            cid: row.cid,
            name: row.name.clone(),
            rating: row.rating.clone(),
            online_hours: row.online_hours,
            active_hours: row.active_hours,
        })
        .collect::<Vec<_>>();

    Ok(ApiJson::new(
        ArtccStatsResponse {
            environment: environment_name,
            label: stats_label(all_time, month_input, selected_year),
            all_time,
            month: month_input,
            year: if all_time { None } else { Some(selected_year) },
            updated_at,
            controller_count,
            summary,
            leaders,
            controllers,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/stats/controller/{cid}/history",
    tag = "stats",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("environment" = Option<String>, Query, description = "Environment: live, sweatbox1, or sweatbox2"),
        ("year" = Option<i32>, Query, description = "Calendar year")
    ),
    responses(
        (status = 200, description = "Controller monthly history", body = ControllerHistoryResponse),
        (status = 400, description = "Invalid query"),
        (status = 404, description = "Controller not found")
    )
)]
pub async fn get_controller_history(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    Query(query): Query<ControllerHistoryQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ControllerHistoryResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let environment = parse_environment(query.environment.as_deref())?;
    let year = query.year.unwrap_or(Utc::now().year());

    if !(2000..=2100).contains(&year) {
        return Err(ApiError::BadRequest);
    }

    let user = stats_repo::fetch_controller_identity(pool, environment.as_str(), cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let rows = stats_repo::list_monthly_buckets(pool, environment.as_str(), cid, year).await?;

    let mut months = (1..=12)
        .map(|month| MonthlyBucket {
            month,
            online_hours: 0.0,
            delivery_hours: 0.0,
            ground_hours: 0.0,
            tower_hours: 0.0,
            tracon_hours: 0.0,
            center_hours: 0.0,
            active_hours: 0.0,
            total_hours: 0.0,
        })
        .collect::<Vec<_>>();

    for row in rows {
        let month_idx = row.month as usize;
        if month_idx < 12 {
            months[month_idx] = MonthlyBucket {
                month: row.month + 1,
                online_hours: row.online_hours,
                delivery_hours: row.delivery_hours,
                ground_hours: row.ground_hours,
                tower_hours: row.tower_hours,
                tracon_hours: row.tracon_hours,
                center_hours: row.center_hours,
                active_hours: row.active_hours,
                total_hours: row.total_hours,
            };
        }
    }

    Ok(ApiJson::new(
        ControllerHistoryResponse {
            environment: environment.as_str().to_string(),
            cid: user.cid,
            name: identity_name(&user),
            rating: normalize_rating_code(user.rating.as_deref()),
            year,
            months,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/stats/controller/{cid}/totals",
    tag = "stats",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("environment" = Option<String>, Query, description = "Environment: live, sweatbox1, or sweatbox2")
    ),
    responses(
        (status = 200, description = "Controller aggregate totals", body = ControllerTotalsResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Controller not found")
    )
)]
pub async fn get_controller_totals(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    Query(query): Query<ControllerHistoryQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ControllerTotalsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let environment = parse_environment(query.environment.as_deref())?;

    let user = stats_repo::fetch_controller_identity(pool, environment.as_str(), cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let totals =
        stats_repo::fetch_controller_totals_aggregate(pool, environment.as_str(), cid).await?;

    let last_activity_at =
        stats_repo::fetch_last_activity_at(pool, environment.as_str(), cid).await?;

    let updated_at = stats_repo::last_feed_updated_at(pool, environment.as_str()).await?;

    Ok(ApiJson::new(
        ControllerTotalsResponse {
            environment: environment.as_str().to_string(),
            cid: user.cid,
            name: identity_name(&user),
            rating: normalize_rating_code(user.rating.as_deref()),
            online_hours: totals.online_hours,
            delivery_hours: totals.delivery_hours,
            ground_hours: totals.ground_hours,
            tower_hours: totals.tower_hours,
            tracon_hours: totals.tracon_hours,
            center_hours: totals.center_hours,
            active_hours: totals.active_hours,
            total_hours: totals.total_hours,
            last_activity_at,
            updated_at,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/stats/controller-events",
    tag = "stats",
    params(
        ("environment" = Option<String>, Query, description = "Environment: live, sweatbox1, or sweatbox2"),
        ("after_id" = Option<i64>, Query, description = "Return events after this durable event id"),
        ("limit" = Option<i64>, Query, description = "Max events to return")
    ),
    responses(
        (status = 200, description = "Durable controller lifecycle events", body = ControllerEventsResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_controller_events(
    State(state): State<AppState>,
    Query(query): Query<ControllerEventsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ControllerEventsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let environment = parse_environment(query.environment.as_deref())?;
    let after_id = query.after_id.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let rows =
        stats_repo::list_controller_events(pool, environment.as_str(), after_id, limit).await?;

    Ok(ApiJson::new(
        ControllerEventsResponse {
            environment: environment.as_str().to_string(),
            events: rows
                .into_iter()
                .map(|row| ControllerEventItem {
                    id: row.id,
                    environment: row.environment,
                    event_type: row.event_type,
                    cid: row.cid,
                    user_id: row.user_id,
                    session_id: row.session_id,
                    activation_id: row.activation_id,
                    occurred_at: row.occurred_at,
                    payload: row.payload,
                })
                .collect(),
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/stats/prefixes",
    tag = "stats",
    responses(
        (status = 200, description = "Statistics prefixes", body = StatisticsPrefixes),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Statistics prefixes not configured")
    )
)]
pub async fn get_statistics_prefixes(
    State(state): State<AppState>,
    _permission: RequirePermission<StatsPrefixesRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<StatisticsPrefixes>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let prefixes = stats_repo::fetch_statistics_prefixes(pool)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(prefixes, time))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/stats/prefixes",
    tag = "stats",
    request_body = UpdateStatisticsPrefixesRequest,
    responses(
        (status = 200, description = "Statistics prefixes updated", body = StatisticsPrefixes),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_statistics_prefixes(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<StatsPrefixesUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateStatisticsPrefixesRequest>,
) -> Result<ApiJson<StatisticsPrefixes>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let prefixes = normalize_prefixes(&payload.prefixes)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let before = stats_repo::fetch_statistics_prefixes(&mut *tx).await?;

    let after = stats_repo::upsert_statistics_prefixes(&mut *tx, &prefixes, Utc::now()).await?;

    let actor = audit_repo::resolve_audit_actor(
        &mut *tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    audit_repo::record_audit(
        &mut *tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "STATISTICS_PREFIXES".to_string(),
            resource_id: Some(after.id.clone()),
            scope_type: "web".to_string(),
            scope_key: Some(after.id.clone()),
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(after, time))
}

fn normalize_prefixes(prefixes: &[String]) -> Result<Vec<String>, ApiError> {
    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::with_capacity(prefixes.len());

    for prefix in prefixes {
        let prefix = prefix.trim().to_ascii_uppercase();
        if prefix.is_empty() {
            return Err(ApiError::BadRequest);
        }
        if seen.insert(prefix.clone()) {
            normalized.push(prefix);
        }
    }

    Ok(normalized)
}

fn row_to_controller_totals(row: &ControllerTotalsRow) -> ControllerTotals {
    ControllerTotals {
        cid: row.cid,
        name: user_name(
            row.display_name.as_deref(),
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.cid,
        ),
        rating: normalize_rating_code(row.rating.as_deref()),
        online_hours: row.online_hours,
        delivery_hours: row.delivery_hours,
        ground_hours: row.ground_hours,
        tower_hours: row.tower_hours,
        tracon_hours: row.tracon_hours,
        center_hours: row.center_hours,
        active_hours: row.active_hours,
        total_hours: row.total_hours,
    }
}

fn identity_name(row: &ControllerIdentityRow) -> String {
    user_name(
        row.display_name.as_deref(),
        row.first_name.as_deref(),
        row.last_name.as_deref(),
        row.cid,
    )
}

fn normalize_rating_code(value: Option<&str>) -> Option<String> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized = match raw.to_ascii_uppercase().as_str() {
        "OBS" | "OBSERVER" => "OBS",
        "S1" | "STUDENT1" => "S1",
        "S2" | "STUDENT2" => "S2",
        "S3" | "STUDENT3" | "SENIORSTUDENT" => "S3",
        "C1" | "CONTROLLER1" => "C1",
        "C2" | "CONTROLLER2" => "C2",
        "C3" | "CONTROLLER3" => "C3",
        "I1" | "I2" | "I3" | "INS" | "INSTRUCTOR" | "INSTRUCTOR1" | "INSTRUCTOR2"
        | "INSTRUCTOR3" => "INS",
        "SUP" | "SUPERVISOR" => "SUP",
        "ADM" | "ADMIN" | "ADMINISTRATOR" => "ADM",
        other => return Some(other.to_string()),
    };

    Some(normalized.to_string())
}

fn user_name(
    display_name: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
    cid: i64,
) -> String {
    let first = first_name.unwrap_or_default().trim();
    let last = last_name.unwrap_or_default().trim();
    let combined = format!("{} {}", first, last).trim().to_string();

    if !combined.is_empty() {
        return combined;
    }

    if let Some(display_name) = display_name
        && !display_name.trim().is_empty()
    {
        return display_name.to_string();
    }

    cid.to_string()
}

fn stats_label(all_time: bool, month: Option<i32>, year: i32) -> String {
    if all_time {
        return "All-Time Statistics".to_string();
    }

    match month {
        Some(value) => format!("{}, {} Statistics", month_name(value), year),
        None => format!("All Months, {} Statistics", year),
    }
}

fn month_name(month: i32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_rating_code, user_name};

    #[test]
    fn normalize_rating_code_returns_short_codes() {
        assert_eq!(
            normalize_rating_code(Some("Student1")).as_deref(),
            Some("S1")
        );
        assert_eq!(
            normalize_rating_code(Some("Student3")).as_deref(),
            Some("S3")
        );
        assert_eq!(
            normalize_rating_code(Some("Supervisor")).as_deref(),
            Some("SUP")
        );
        assert_eq!(
            normalize_rating_code(Some("Instructor1")).as_deref(),
            Some("INS")
        );
        assert_eq!(normalize_rating_code(Some("C1")).as_deref(), Some("C1"));
        assert_eq!(normalize_rating_code(Some("")).as_deref(), None);
    }

    #[test]
    fn user_name_prefers_joined_first_and_last_name() {
        assert_eq!(
            user_name(
                Some("Display Name"),
                Some("Jane"),
                Some("Controller"),
                10000001
            ),
            "Jane Controller"
        );
    }

    #[test]
    fn user_name_uses_non_empty_single_name_part() {
        assert_eq!(
            user_name(Some("Display Name"), Some("Jane"), Some("   "), 10000001),
            "Jane"
        );
        assert_eq!(
            user_name(
                Some("Display Name"),
                Some("   "),
                Some("Controller"),
                10000001
            ),
            "Controller"
        );
    }

    #[test]
    fn user_name_falls_back_to_display_name() {
        assert_eq!(
            user_name(Some("Display Name"), Some("   "), Some(""), 10000001),
            "Display Name"
        );
    }

    #[test]
    fn user_name_falls_back_to_cid_when_identity_name_missing() {
        assert_eq!(
            user_name(Some("   "), Some(""), Some(""), 10000001),
            "10000001"
        );
        assert_eq!(user_name(None, None, None, 10000001), "10000001");
    }
}
