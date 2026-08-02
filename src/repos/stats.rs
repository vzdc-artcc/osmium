use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    errors::ApiError,
    models::{ControllerPositionItem, StatisticsPrefixes},
};

const STATISTICS_PREFIXES_ID: &str = "default";

#[derive(sqlx::FromRow)]
pub struct ControllerTotalsRow {
    pub cid: i64,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
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
pub struct ControllerIdentityRow {
    pub cid: i64,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub rating: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct MonthlyBucketRow {
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

#[derive(sqlx::FromRow)]
pub struct ControllerTotalsAggregateRow {
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
pub struct ControllerEventRow {
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

pub async fn count_controllers(
    pool: &PgPool,
    environment: &str,
    all_time: bool,
    year: i32,
    month_zero_based: Option<i32>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(distinct cid)::bigint
        from stats.controller_monthly_rollups
        where environment = $1
          and ($2::boolean = true or year = $3)
          and ($4::int is null or month = $4)
        "#,
    )
    .bind(environment)
    .bind(all_time)
    .bind(year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_artcc_summary(
    pool: &PgPool,
    environment: &str,
    all_time: bool,
    year: i32,
    month_zero_based: Option<i32>,
) -> Result<crate::models::stats::ArtccSummary, ApiError> {
    sqlx::query_as::<_, crate::models::stats::ArtccSummary>(
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
    .bind(environment)
    .bind(all_time)
    .bind(year)
    .bind(month_zero_based)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_controller_totals_rows(
    pool: &PgPool,
    environment: &str,
    all_time: bool,
    year: i32,
    month_zero_based: Option<i32>,
    query_limit: i64,
) -> Result<Vec<ControllerTotalsRow>, ApiError> {
    sqlx::query_as::<_, ControllerTotalsRow>(
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
    .bind(environment)
    .bind(all_time)
    .bind(year)
    .bind(month_zero_based)
    .bind(query_limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_monthly_buckets(
    pool: &PgPool,
    environment: &str,
    cid: i64,
    year: i32,
) -> Result<Vec<MonthlyBucketRow>, ApiError> {
    sqlx::query_as::<_, MonthlyBucketRow>(
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
    .bind(environment)
    .bind(cid)
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_controller_positions(
    pool: &PgPool,
    environment: &str,
    cid: i64,
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from stats.controller_activations
        where environment = $1 and cid = $2
          and ($3::timestamptz is null or started_at >= $3)
          and ($4::timestamptz is null or started_at < $4)
        "#,
    )
    .bind(environment)
    .bind(cid)
    .bind(range.map(|(start, _)| start))
    .bind(range.map(|(_, end)| end))
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_controller_positions(
    pool: &PgPool,
    environment: &str,
    cid: i64,
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<ControllerPositionItem>, ApiError> {
    sqlx::query_as::<_, ControllerPositionItem>(
        r#"
        select
            position_name,
            facility_name,
            is_primary,
            started_at,
            ended_at,
            active_seconds
        from stats.controller_activations
        where environment = $1 and cid = $2
          and ($5::timestamptz is null or started_at >= $5)
          and ($6::timestamptz is null or started_at < $6)
        order by started_at desc
        limit $3 offset $4
        "#,
    )
    .bind(environment)
    .bind(cid)
    .bind(page_size)
    .bind(offset)
    .bind(range.map(|(start, _)| start))
    .bind(range.map(|(_, end)| end))
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_artcc_monthly_buckets(
    pool: &PgPool,
    environment: &str,
    year: i32,
) -> Result<Vec<MonthlyBucketRow>, ApiError> {
    sqlx::query_as::<_, MonthlyBucketRow>(
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
        where environment = $1 and year = $2
        group by month
        order by month asc
        "#,
    )
    .bind(environment)
    .bind(year)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_controller_totals_aggregate(
    pool: &PgPool,
    environment: &str,
    cid: i64,
) -> Result<ControllerTotalsAggregateRow, ApiError> {
    sqlx::query_as::<_, ControllerTotalsAggregateRow>(
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
    .bind(environment)
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Date-ranged controller totals, summed from individual activations (filtered
/// by `started_at`) rather than the all-time monthly rollups. Facility buckets
/// mirror `stats_sync::map_position_type` so the numbers stay consistent with
/// the rollup-based totals. `online_hours` counts all activations (incl.
/// uncategorized), while the facility hours and `active/total_hours` count only
/// the DEL/GND/TWR/TRACON/CTR positions.
pub async fn fetch_controller_totals_aggregate_ranged(
    pool: &PgPool,
    environment: &str,
    cid: i64,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<ControllerTotalsAggregateRow, ApiError> {
    sqlx::query_as::<_, ControllerTotalsAggregateRow>(
        r#"
        select
            coalesce(sum(active_seconds), 0)::float8 / 3600.0 as online_hours,
            coalesce(sum(active_seconds) filter (where position_type in ('Delivery', 'ClearanceDelivery')), 0)::float8 / 3600.0 as delivery_hours,
            coalesce(sum(active_seconds) filter (where position_type = 'Ground'), 0)::float8 / 3600.0 as ground_hours,
            coalesce(sum(active_seconds) filter (where position_type = 'Tower'), 0)::float8 / 3600.0 as tower_hours,
            coalesce(sum(active_seconds) filter (where position_type in ('Tracon', 'ApproachDeparture')), 0)::float8 / 3600.0 as tracon_hours,
            coalesce(sum(active_seconds) filter (where position_type in ('Artcc', 'Center')), 0)::float8 / 3600.0 as center_hours,
            coalesce(sum(active_seconds) filter (where position_type in ('Delivery', 'ClearanceDelivery', 'Ground', 'Tower', 'Tracon', 'ApproachDeparture', 'Artcc', 'Center')), 0)::float8 / 3600.0 as active_hours,
            coalesce(sum(active_seconds) filter (where position_type in ('Delivery', 'ClearanceDelivery', 'Ground', 'Tower', 'Tracon', 'ApproachDeparture', 'Artcc', 'Center')), 0)::float8 / 3600.0 as total_hours
        from stats.controller_activations
        where environment = $1 and cid = $2
          and ($3::timestamptz is null or started_at >= $3)
          and ($4::timestamptz is null or started_at < $4)
        "#,
    )
    .bind(environment)
    .bind(cid)
    .bind(since)
    .bind(until)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_last_activity_at(
    pool: &PgPool,
    environment: &str,
    cid: i64,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        select nullif(
            greatest(
                coalesce((select max(coalesce(ended_at, started_at)) from stats.controller_activations where environment = $1 and cid = $2), '-infinity'::timestamptz),
                coalesce((select max(coalesce(logout_at, login_at)) from stats.controller_sessions where environment = $1 and cid = $2), '-infinity'::timestamptz)
            ),
            '-infinity'::timestamptz
        )
        "#,
    )
    .bind(environment)
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_controller_events(
    pool: &PgPool,
    environment: &str,
    after_id: i64,
    limit: i64,
) -> Result<Vec<ControllerEventRow>, ApiError> {
    sqlx::query_as::<_, ControllerEventRow>(
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
    .bind(environment)
    .bind(after_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_controller_identity(
    pool: &PgPool,
    environment: &str,
    cid: i64,
) -> Result<Option<ControllerIdentityRow>, ApiError> {
    sqlx::query_as::<_, ControllerIdentityRow>(
        r#"
        select
            coalesce(p.cid, latest.cid, input.cid) as cid,
            p.display_name,
            p.first_name,
            p.last_name,
            coalesce(p.rating, latest.user_rating, latest.requested_rating) as rating
        from (select $2::bigint as cid) input
        left join lateral (
            select cid, user_rating, requested_rating
            from stats.controller_sessions
            where environment = $1 and cid = $2
            order by login_at desc
            limit 1
        ) latest on true
        left join org.v_user_roster_profile p on p.cid = coalesce(latest.cid, input.cid)
        where p.cid is not null or latest.cid is not null
        "#,
    )
    .bind(environment)
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn last_feed_updated_at(
    pool: &PgPool,
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

pub async fn list_online_controllers(
    pool: &PgPool,
) -> Result<Vec<crate::models::OnlineControllerItem>, ApiError> {
    // Live-environment controllers currently connected: an open session
    // (logout_at is null) with a currently-active position (ended_at is null).
    // Sessions/activations are the post-0017 model that the stats sync writes;
    // the old `controller_positions` table it replaced is no longer populated.
    sqlx::query_as::<_, crate::models::OnlineControllerItem>(
        r#"
        select
            s.cid,
            coalesce(u.display_name, s.real_name, s.cid::text) as display_name,
            coalesce(m.rating, s.user_rating) as rating,
            coalesce(a.default_callsign, a.position_name) as position,
            a.started_at as start
        from stats.controller_sessions s
        join stats.controller_activations a
            on a.session_id = s.id and a.ended_at is null
        left join identity.users u on u.id = s.user_id
        left join org.memberships m on m.user_id = u.id
        where s.logout_at is null
          and s.environment = 'live'
        order by a.started_at asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_statistics_prefixes<'e, E>(
    executor: E,
) -> Result<Option<StatisticsPrefixes>, ApiError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, StatisticsPrefixes>(
        "select id, prefixes, updated_at from stats.statistics_prefixes where id = $1",
    )
    .bind(STATISTICS_PREFIXES_ID)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn upsert_statistics_prefixes<'e, E>(
    executor: E,
    prefixes: &[String],
    now: DateTime<Utc>,
) -> Result<StatisticsPrefixes, ApiError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, StatisticsPrefixes>(
        r#"
        insert into stats.statistics_prefixes (id, prefixes, updated_at)
        values ($1, $2, $3)
        on conflict (id) do update
        set prefixes = excluded.prefixes,
            updated_at = excluded.updated_at
        returning id, prefixes, updated_at
        "#,
    )
    .bind(STATISTICS_PREFIXES_ID)
    .bind(prefixes)
    .bind(now)
    .fetch_one(executor)
    .await
    .map_err(|_| ApiError::Internal)
}
