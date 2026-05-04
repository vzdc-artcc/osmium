use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{errors::ApiError, jobs::stats_sync::parse_environment, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct ArtccStatsQuery {
    pub environment: Option<String>,
    pub all_time: Option<bool>,
    pub month: Option<i32>,
    pub year: Option<i32>,
    pub top: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ArtccStatsResponse {
    pub environment: String,
    pub label: String,
    pub all_time: bool,
    pub month: Option<i32>,
    pub year: Option<i32>,
    pub updated_at: Option<DateTime<Utc>>,
    pub controller_count: i64,
    pub summary: ArtccSummary,
    pub leaders: Vec<ControllerLeader>,
    pub controllers: Vec<ControllerTotals>,
}

#[derive(Deserialize, ToSchema)]
pub struct ControllerHistoryQuery {
    pub environment: Option<String>,
    pub year: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerHistoryResponse {
    pub environment: String,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub year: i32,
    pub months: Vec<MonthlyBucket>,
}

#[derive(Deserialize, ToSchema)]
pub struct ControllerEventsQuery {
    pub environment: Option<String>,
    pub after_id: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerEventsResponse {
    pub environment: String,
    pub events: Vec<ControllerEventItem>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerEventItem {
    pub id: i64,
    pub environment: String,
    pub event_type: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub activation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerTotalsResponse {
    pub environment: String,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct MonthlyBucket {
    pub month: i32,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct ArtccSummary {
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerLeader {
    pub rank: i32,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub active_hours: f64,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct ControllerTotals {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerTotalsRow {
    cid: i64,
    display_name: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    rating: Option<String>,
    session_real_name: Option<String>,
    online_hours: f64,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    active_hours: f64,
    total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerIdentityRow {
    cid: i64,
    display_name: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    rating: Option<String>,
    session_real_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct MonthlyBucketRow {
    month: i32,
    online_hours: f64,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    active_hours: f64,
    total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerTotalsAggregateRow {
    online_hours: f64,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    active_hours: f64,
    total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerEventRow {
    id: i64,
    environment: String,
    event_type: String,
    cid: i64,
    user_id: Option<String>,
    session_id: Option<String>,
    activation_id: Option<String>,
    occurred_at: DateTime<Utc>,
    payload: Value,
}

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
) -> Result<Json<ArtccStatsResponse>, ApiError> {
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

    let updated_at = last_feed_updated_at(pool, environment.as_str()).await?;

    let controller_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(distinct cid)::bigint
        from stats.controller_monthly_rollups
        where environment = $1
          and ($2::boolean = true or year = $3)
          and ($4::int is null or month = $4)
        "#,
    )
    .bind(environment.as_str())
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let summary = sqlx::query_as::<_, ArtccSummary>(
        r#"
        select
            coalesce(sum(online_seconds), 0)::float8 / 3600.0 as online_hours,
            coalesce(sum(delivery_seconds), 0)::float8 / 3600.0 as delivery_hours,
            coalesce(sum(ground_seconds), 0)::float8 / 3600.0 as ground_hours,
            coalesce(sum(tower_seconds), 0)::float8 / 3600.0 as tower_hours,
            coalesce(sum(tracon_seconds), 0)::float8 / 3600.0 as tracon_hours,
            coalesce(sum(center_seconds), 0)::float8 / 3600.0 as center_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as active_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as total_hours
        from stats.controller_monthly_rollups
        where environment = $1
          and ($2::boolean = true or year = $3)
          and ($4::int is null or month = $4)
        "#,
    )
    .bind(environment.as_str())
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let controller_rows = sqlx::query_as::<_, ControllerTotalsRow>(
        r#"
        with rollups as (
            select
                cid,
                coalesce(sum(online_seconds), 0)::float8 / 3600.0 as online_hours,
                coalesce(sum(delivery_seconds), 0)::float8 / 3600.0 as delivery_hours,
                coalesce(sum(ground_seconds), 0)::float8 / 3600.0 as ground_hours,
                coalesce(sum(tower_seconds), 0)::float8 / 3600.0 as tower_hours,
                coalesce(sum(tracon_seconds), 0)::float8 / 3600.0 as tracon_hours,
                coalesce(sum(center_seconds), 0)::float8 / 3600.0 as center_hours,
                (
                    coalesce(sum(delivery_seconds), 0) +
                    coalesce(sum(ground_seconds), 0) +
                    coalesce(sum(tower_seconds), 0) +
                    coalesce(sum(tracon_seconds), 0) +
                    coalesce(sum(center_seconds), 0)
                )::float8 / 3600.0 as active_hours
            from stats.controller_monthly_rollups
            where environment = $1
              and ($2::boolean = true or year = $3)
              and ($4::int is null or month = $4)
            group by cid
        )
        select
            r.cid,
            p.display_name,
            p.first_name,
            p.last_name,
            coalesce(p.rating, latest.user_rating, latest.requested_rating) as rating,
            latest.real_name as session_real_name,
            r.online_hours,
            r.delivery_hours,
            r.ground_hours,
            r.tower_hours,
            r.tracon_hours,
            r.center_hours,
            r.active_hours,
            r.active_hours as total_hours
        from rollups r
        left join org.v_user_roster_profile p on p.cid = r.cid
        left join lateral (
            select real_name, user_rating, requested_rating
            from stats.controller_sessions
            where environment = $1 and cid = r.cid
            order by login_at desc
            limit 1
        ) latest on true
        where r.online_hours > 0 or r.active_hours > 0
        order by r.online_hours desc, r.cid asc
        limit $5
        "#,
    )
    .bind(environment.as_str())
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .bind(query_limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

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

    Ok(Json(ArtccStatsResponse {
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
    }))
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
        (status = 400, description = "Invalid query")
    )
)]
pub async fn get_controller_history(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    Query(query): Query<ControllerHistoryQuery>,
) -> Result<Json<ControllerHistoryResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let environment = parse_environment(query.environment.as_deref())?;
    let year = query.year.unwrap_or(Utc::now().year());

    if !(2000..=2100).contains(&year) {
        return Err(ApiError::BadRequest);
    }

    let user = fetch_controller_identity(pool, environment.as_str(), cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let rows = sqlx::query_as::<_, MonthlyBucketRow>(
        r#"
        select
            month,
            coalesce(sum(online_seconds), 0)::float8 / 3600.0 as online_hours,
            coalesce(sum(delivery_seconds), 0)::float8 / 3600.0 as delivery_hours,
            coalesce(sum(ground_seconds), 0)::float8 / 3600.0 as ground_hours,
            coalesce(sum(tower_seconds), 0)::float8 / 3600.0 as tower_hours,
            coalesce(sum(tracon_seconds), 0)::float8 / 3600.0 as tracon_hours,
            coalesce(sum(center_seconds), 0)::float8 / 3600.0 as center_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as active_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as total_hours
        from stats.controller_monthly_rollups
        where environment = $1 and cid = $2 and year = $3
        group by month
        order by month asc
        "#,
    )
    .bind(environment.as_str())
    .bind(cid)
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

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

    Ok(Json(ControllerHistoryResponse {
        environment: environment.as_str().to_string(),
        cid: user.cid,
        name: identity_name(&user),
        rating: normalize_rating_code(user.rating.as_deref()),
        year,
        months,
    }))
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
        (status = 400, description = "Invalid request")
    )
)]
pub async fn get_controller_totals(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    Query(query): Query<ControllerHistoryQuery>,
) -> Result<Json<ControllerTotalsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let environment = parse_environment(query.environment.as_deref())?;

    let user = fetch_controller_identity(pool, environment.as_str(), cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let totals = sqlx::query_as::<_, ControllerTotalsAggregateRow>(
        r#"
        select
            coalesce(sum(online_seconds), 0)::float8 / 3600.0 as online_hours,
            coalesce(sum(delivery_seconds), 0)::float8 / 3600.0 as delivery_hours,
            coalesce(sum(ground_seconds), 0)::float8 / 3600.0 as ground_hours,
            coalesce(sum(tower_seconds), 0)::float8 / 3600.0 as tower_hours,
            coalesce(sum(tracon_seconds), 0)::float8 / 3600.0 as tracon_hours,
            coalesce(sum(center_seconds), 0)::float8 / 3600.0 as center_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as active_hours,
            (
                coalesce(sum(delivery_seconds), 0) +
                coalesce(sum(ground_seconds), 0) +
                coalesce(sum(tower_seconds), 0) +
                coalesce(sum(tracon_seconds), 0) +
                coalesce(sum(center_seconds), 0)
            )::float8 / 3600.0 as total_hours
        from stats.controller_monthly_rollups
        where environment = $1 and cid = $2
        "#,
    )
    .bind(environment.as_str())
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let last_activity_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        select greatest(
            coalesce((select max(coalesce(ended_at, started_at)) from stats.controller_activations where environment = $1 and cid = $2), '-infinity'::timestamptz),
            coalesce((select max(coalesce(logout_at, login_at)) from stats.controller_sessions where environment = $1 and cid = $2), '-infinity'::timestamptz)
        )
        "#,
    )
    .bind(environment.as_str())
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let updated_at = last_feed_updated_at(pool, environment.as_str()).await?;

    Ok(Json(ControllerTotalsResponse {
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
    }))
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
) -> Result<Json<ControllerEventsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let environment = parse_environment(query.environment.as_deref())?;
    let after_id = query.after_id.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query_as::<_, ControllerEventRow>(
        r#"
        select
            id,
            environment,
            event_type,
            cid,
            user_id,
            session_id,
            activation_id,
            occurred_at,
            payload
        from stats.controller_events
        where environment = $1 and id > $2
        order by id asc
        limit $3
        "#,
    )
    .bind(environment.as_str())
    .bind(after_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(ControllerEventsResponse {
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
    }))
}

async fn fetch_controller_identity(
    pool: &sqlx::PgPool,
    environment: &str,
    cid: i64,
) -> Result<Option<ControllerIdentityRow>, ApiError> {
    sqlx::query_as::<_, ControllerIdentityRow>(
        r#"
        select
            coalesce(p.cid, latest.cid) as cid,
            p.display_name,
            p.first_name,
            p.last_name,
            coalesce(p.rating, latest.user_rating, latest.requested_rating) as rating,
            latest.real_name as session_real_name
        from (
            select cid, real_name, user_rating, requested_rating
            from stats.controller_sessions
            where environment = $1 and cid = $2
            order by login_at desc
            limit 1
        ) latest
        left join org.v_user_roster_profile p on p.cid = latest.cid
        "#,
    )
    .bind(environment)
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn last_feed_updated_at(
    pool: &sqlx::PgPool,
    environment: &str,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "select last_source_updated_at from stats.controller_feed_state where environment = $1",
    )
    .bind(environment)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
    .map(|value| value.flatten())
}

fn row_to_controller_totals(row: &ControllerTotalsRow) -> ControllerTotals {
    ControllerTotals {
        cid: row.cid,
        name: user_name(
            row.display_name.as_deref(),
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.session_real_name.as_deref(),
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
        row.session_real_name.as_deref(),
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
    session_real_name: Option<&str>,
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

    if let Some(session_real_name) = session_real_name
        && !session_real_name.trim().is_empty()
    {
        return session_real_name.to_string();
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
    use super::normalize_rating_code;

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
}
