use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::errors::ApiError;

/// One roster member's activity summary for the Purge Assistant's selection
/// table — bounded to a `[year, start_month..end_month]` window (both
/// zero-indexed, matching `stats.controller_monthly_rollups.month` and the
/// legacy website's own `getMonth()`-based params) for controlling/training
/// hours, and to `< period_end_exclusive` for join-date/broadcast checks.
/// Ports the legacy `/admin/purge-assistant` page's selection query, with
/// one deliberate correction: "has an approved LOA" is evaluated as
/// *currently active* (`start <= now <= end`), not "has ever had one
/// approved" — the latter was almost certainly a bug in the original Prisma
/// `every` clause (an LOA approved a year ago shouldn't protect someone
/// from being purged today).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PurgeCandidateRow {
    pub cid: i64,
    pub display_name: String,
    pub email: String,
    pub rating: Option<String>,
    pub controller_status: String,
    pub join_date: DateTime<Utc>,
    pub controlling_hours: f64,
    pub trainer_hours_given: f64,
    pub trainer_hours_received: f64,
    pub open_broadcasts: i64,
    pub has_active_approved_loa: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn list_purge_candidates(
    pool: &PgPool,
    environment: &str,
    year: i32,
    start_month: i32,
    end_month: i32,
    period_start: DateTime<Utc>,
    period_end_exclusive: DateTime<Utc>,
) -> Result<Vec<PurgeCandidateRow>, ApiError> {
    sqlx::query_as::<_, PurgeCandidateRow>(
        r#"
        with hours as (
            select
                cid,
                coalesce(sum(delivery_seconds + ground_seconds + tower_seconds + tracon_seconds + center_seconds), 0)::float8 / 3600.0 as controlling_hours
            from stats.controller_monthly_rollups
            where environment = $1
              and year = $2
              and month between $3 and $4
            group by cid
        ),
        trainer_given as (
            select
                instructor_id as user_id,
                coalesce(sum(extract(epoch from ("end" - start))), 0)::float8 / 3600.0 as hours
            from training.training_sessions
            where start >= $5 and start < $6
            group by instructor_id
        ),
        trainer_received as (
            select
                student_id as user_id,
                coalesce(sum(extract(epoch from ("end" - start))), 0)::float8 / 3600.0 as hours
            from training.training_sessions
            where start >= $5 and start < $6
            group by student_id
        ),
        unseen_broadcasts as (
            select r.user_id, count(*)::bigint as open_count
            from web.change_broadcast_recipients r
            join web.change_broadcasts b on b.id = r.broadcast_id
            left join web.change_broadcast_user_state s
                on s.broadcast_id = r.broadcast_id and s.user_id = r.user_id
            where s.seen_at is null and b.timestamp < $6
            group by r.user_id
        ),
        active_loas as (
            select distinct user_id
            from org.loas
            where status = 'APPROVED' and start <= now() and "end" >= now()
        )
        select
            u.cid,
            u.display_name,
            u.email,
            m.rating,
            m.controller_status,
            m.join_date,
            coalesce(h.controlling_hours, 0) as controlling_hours,
            coalesce(tg.hours, 0) as trainer_hours_given,
            coalesce(tr.hours, 0) as trainer_hours_received,
            coalesce(ub.open_count, 0) as open_broadcasts,
            (al.user_id is not null) as has_active_approved_loa
        from org.memberships m
        join identity.users u on u.id = m.user_id
        left join hours h on h.cid = u.cid
        left join trainer_given tg on tg.user_id = m.user_id
        left join trainer_received tr on tr.user_id = m.user_id
        left join unseen_broadcasts ub on ub.user_id = m.user_id
        left join active_loas al on al.user_id = m.user_id
        where m.controller_status <> 'NONE'
          and m.join_date < $6
          and not (
              m.rating = 'OBS'
              and exists (
                  select 1 from training.training_assignment_requests tar
                  where tar.student_id = m.user_id and tar.status = 'PENDING'
              )
          )
        order by u.display_name asc
        "#,
    )
    .bind(environment)
    .bind(year)
    .bind(start_month)
    .bind(end_month)
    .bind(period_start)
    .bind(period_end_exclusive)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
