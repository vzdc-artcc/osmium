use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

use crate::{errors::ApiError, state::AppState};

#[derive(Deserialize)]
pub struct ArtccStatsQuery {
    pub all_time: Option<bool>,
    pub month: Option<i32>, // 1-12
    pub year: Option<i32>,
    pub top: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct ArtccStatsResponse {
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

#[derive(Deserialize)]
pub struct ControllerHistoryQuery {
    pub year: Option<i32>,
}

#[derive(Serialize)]
pub struct ControllerHistoryResponse {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub year: i32,
    pub months: Vec<MonthlyBucket>,
}

#[derive(Serialize)]
pub struct ControllerTotalsResponse {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub total_hours: f64,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Clone)]
pub struct MonthlyBucket {
    pub month: i32, // 1-12
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ArtccSummary {
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize)]
pub struct ControllerLeader {
    pub rank: i32,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub total_hours: f64,
}

#[derive(Serialize, Clone)]
pub struct ControllerTotals {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerTotalsRow {
    cid: i64,
    display_name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    rating: Option<String>,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerHistoryUserRow {
    cid: i64,
    display_name: String,
    first_name: Option<String>,
    last_name: Option<String>,
    rating: Option<String>,
}

#[derive(sqlx::FromRow)]
struct MonthlyBucketRow {
    month: i32,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    total_hours: f64,
}

#[derive(sqlx::FromRow)]
struct ControllerTotalsAggregateRow {
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    tracon_hours: f64,
    center_hours: f64,
    total_hours: f64,
}

pub async fn get_artcc_stats(
    State(state): State<AppState>,
    Query(query): Query<ArtccStatsQuery>,
) -> Result<Json<ArtccStatsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

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

    let updated_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "select stats from sync_times where id = 'default'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .flatten();

    let controller_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(distinct clm.log_id)::bigint
        from controller_log_months clm
        where ($1::boolean = true or clm.year = $2)
          and ($3::int is null or clm.month = $3)
        "#,
    )
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let summary = sqlx::query_as::<_, ArtccSummary>(
        r#"
        select
            coalesce(sum(clm.delivery_hours), 0)::float8 as delivery_hours,
            coalesce(sum(clm.ground_hours), 0)::float8 as ground_hours,
            coalesce(sum(clm.tower_hours), 0)::float8 as tower_hours,
            coalesce(sum(clm.approach_hours), 0)::float8 as tracon_hours,
            coalesce(sum(clm.center_hours), 0)::float8 as center_hours,
            (
                coalesce(sum(clm.delivery_hours), 0) +
                coalesce(sum(clm.ground_hours), 0) +
                coalesce(sum(clm.tower_hours), 0) +
                coalesce(sum(clm.approach_hours), 0) +
                coalesce(sum(clm.center_hours), 0)
            )::float8 as total_hours
        from controller_log_months clm
        where ($1::boolean = true or clm.year = $2)
          and ($3::int is null or clm.month = $3)
        "#,
    )
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let controller_rows = sqlx::query_as::<_, ControllerTotalsRow>(
        r#"
        select
            u.cid,
            u.display_name,
            u.first_name,
            u.last_name,
            u.rating,
            coalesce(sum(clm.delivery_hours), 0)::float8 as delivery_hours,
            coalesce(sum(clm.ground_hours), 0)::float8 as ground_hours,
            coalesce(sum(clm.tower_hours), 0)::float8 as tower_hours,
            coalesce(sum(clm.approach_hours), 0)::float8 as tracon_hours,
            coalesce(sum(clm.center_hours), 0)::float8 as center_hours,
            (
                coalesce(sum(clm.delivery_hours), 0) +
                coalesce(sum(clm.ground_hours), 0) +
                coalesce(sum(clm.tower_hours), 0) +
                coalesce(sum(clm.approach_hours), 0) +
                coalesce(sum(clm.center_hours), 0)
            )::float8 as total_hours
        from controller_log_months clm
        join controller_logs cl on cl.id = clm.log_id
        join users u on u.id = cl.user_id
        where ($1::boolean = true or clm.year = $2)
          and ($3::int is null or clm.month = $3)
        group by u.cid, u.display_name, u.first_name, u.last_name, u.rating
        having (
            coalesce(sum(clm.delivery_hours), 0) +
            coalesce(sum(clm.ground_hours), 0) +
            coalesce(sum(clm.tower_hours), 0) +
            coalesce(sum(clm.approach_hours), 0) +
            coalesce(sum(clm.center_hours), 0)
        ) > 0
        order by total_hours desc, u.cid asc
        limit $4
        "#,
    )
    .bind(all_time)
    .bind(selected_year)
    .bind(month_zero_based)
    .bind(query_limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let all_controllers: Vec<ControllerTotals> = controller_rows
        .iter()
        .map(|row| ControllerTotals {
            cid: row.cid,
            name: user_name(&row.display_name, row.first_name.as_deref(), row.last_name.as_deref()),
            rating: row.rating.clone(),
            delivery_hours: row.delivery_hours,
            ground_hours: row.ground_hours,
            tower_hours: row.tower_hours,
            tracon_hours: row.tracon_hours,
            center_hours: row.center_hours,
            total_hours: row.total_hours,
        })
        .collect();

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
            total_hours: row.total_hours,
        })
        .collect();

    Ok(Json(ArtccStatsResponse {
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

pub async fn get_controller_history(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    Query(query): Query<ControllerHistoryQuery>,
) -> Result<Json<ControllerHistoryResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let now = Utc::now();
    let year = query.year.unwrap_or(now.year());

    if !(2000..=2100).contains(&year) {
        return Err(ApiError::BadRequest);
    }

    let user = sqlx::query_as::<_, ControllerHistoryUserRow>(
        r#"
        select cid, display_name, first_name, last_name, rating
        from users
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let rows = sqlx::query_as::<_, MonthlyBucketRow>(
        r#"
        select
            clm.month,
            coalesce(sum(clm.delivery_hours), 0)::float8 as delivery_hours,
            coalesce(sum(clm.ground_hours), 0)::float8 as ground_hours,
            coalesce(sum(clm.tower_hours), 0)::float8 as tower_hours,
            coalesce(sum(clm.approach_hours), 0)::float8 as tracon_hours,
            coalesce(sum(clm.center_hours), 0)::float8 as center_hours,
            (
                coalesce(sum(clm.delivery_hours), 0) +
                coalesce(sum(clm.ground_hours), 0) +
                coalesce(sum(clm.tower_hours), 0) +
                coalesce(sum(clm.approach_hours), 0) +
                coalesce(sum(clm.center_hours), 0)
            )::float8 as total_hours
        from controller_log_months clm
        join controller_logs cl on cl.id = clm.log_id
        join users u on u.id = cl.user_id
        where u.cid = $1 and clm.year = $2
        group by clm.month
        order by clm.month asc
        "#,
    )
    .bind(cid)
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut months = (1..=12)
        .map(|month| MonthlyBucket {
            month,
            delivery_hours: 0.0,
            ground_hours: 0.0,
            tower_hours: 0.0,
            tracon_hours: 0.0,
            center_hours: 0.0,
            total_hours: 0.0,
        })
        .collect::<Vec<_>>();

    for row in rows {
        let month_idx = row.month as usize;
        if month_idx < 12 {
            months[month_idx] = MonthlyBucket {
                month: row.month + 1,
                delivery_hours: row.delivery_hours,
                ground_hours: row.ground_hours,
                tower_hours: row.tower_hours,
                tracon_hours: row.tracon_hours,
                center_hours: row.center_hours,
                total_hours: row.total_hours,
            };
        }
    }

    Ok(Json(ControllerHistoryResponse {
        cid: user.cid,
        name: user_name(
            &user.display_name,
            user.first_name.as_deref(),
            user.last_name.as_deref(),
        ),
        rating: user.rating,
        year,
        months,
    }))
}

pub async fn get_controller_totals(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
) -> Result<Json<ControllerTotalsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let user = sqlx::query_as::<_, ControllerHistoryUserRow>(
        r#"
        select cid, display_name, first_name, last_name, rating
        from users
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let totals = sqlx::query_as::<_, ControllerTotalsAggregateRow>(
        r#"
        select
            coalesce(sum(clm.delivery_hours), 0)::float8 as delivery_hours,
            coalesce(sum(clm.ground_hours), 0)::float8 as ground_hours,
            coalesce(sum(clm.tower_hours), 0)::float8 as tower_hours,
            coalesce(sum(clm.approach_hours), 0)::float8 as tracon_hours,
            coalesce(sum(clm.center_hours), 0)::float8 as center_hours,
            (
                coalesce(sum(clm.delivery_hours), 0) +
                coalesce(sum(clm.ground_hours), 0) +
                coalesce(sum(clm.tower_hours), 0) +
                coalesce(sum(clm.approach_hours), 0) +
                coalesce(sum(clm.center_hours), 0)
            )::float8 as total_hours
        from controller_log_months clm
        join controller_logs cl on cl.id = clm.log_id
        join users u on u.id = cl.user_id
        where u.cid = $1
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let last_activity_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        select max(coalesce(cp."end", cp.start))
        from controller_positions cp
        join controller_logs cl on cl.id = cp.log_id
        join users u on u.id = cl.user_id
        where u.cid = $1
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let updated_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "select stats from sync_times where id = 'default'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .flatten();

    Ok(Json(ControllerTotalsResponse {
        cid: user.cid,
        name: user_name(
            &user.display_name,
            user.first_name.as_deref(),
            user.last_name.as_deref(),
        ),
        rating: user.rating,
        delivery_hours: totals.delivery_hours,
        ground_hours: totals.ground_hours,
        tower_hours: totals.tower_hours,
        tracon_hours: totals.tracon_hours,
        center_hours: totals.center_hours,
        total_hours: totals.total_hours,
        last_activity_at,
        updated_at,
    }))
}

fn user_name(display_name: &str, first_name: Option<&str>, last_name: Option<&str>) -> String {
    let first = first_name.unwrap_or_default().trim();
    let last = last_name.unwrap_or_default().trim();
    let combined = format!("{} {}", first, last).trim().to_string();

    if combined.is_empty() {
        display_name.to_string()
    } else {
        combined
    }
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

