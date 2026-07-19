use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{errors::ApiError, models::incidents::IncidentItem};

#[derive(Debug, sqlx::FromRow)]
struct IncidentItemRow {
    id: String,
    reporter_id: String,
    reportee_id: String,
    timestamp: DateTime<Utc>,
    reason: String,
    closed: bool,
    reporter_callsign: Option<String>,
    reportee_callsign: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    reporter_cid: Option<i64>,
    reporter_name: Option<String>,
    reportee_cid: Option<i64>,
    reportee_name: Option<String>,
}

impl From<IncidentItemRow> for IncidentItem {
    fn from(row: IncidentItemRow) -> Self {
        IncidentItem {
            id: row.id,
            reporter_id: row.reporter_id,
            reportee_id: row.reportee_id,
            timestamp: row.timestamp,
            reason: row.reason,
            closed: row.closed,
            reporter_callsign: row.reporter_callsign,
            reportee_callsign: row.reportee_callsign,
            created_at: row.created_at,
            updated_at: row.updated_at,
            reporter_cid: row.reporter_cid,
            reporter_name: row.reporter_name,
            reportee_cid: row.reportee_cid,
            reportee_name: row.reportee_name,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_incident(
    pool: &PgPool,
    id: &str,
    reporter_id: &str,
    reportee_id: &str,
    timestamp: DateTime<Utc>,
    reason: &str,
    reporter_callsign: Option<&str>,
    reportee_callsign: &str,
) -> Result<IncidentItem, ApiError> {
    sqlx::query_as::<_, IncidentItemRow>(
        r#"
        insert into feedback.incident_reports (
            id,
            reporter_id,
            reportee_id,
            timestamp,
            reason,
            closed,
            reporter_callsign,
            reportee_callsign,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, false, $6, $7, now(), now())
        returning
            id,
            reporter_id,
            reportee_id,
            timestamp,
            reason,
            closed,
            reporter_callsign,
            reportee_callsign,
            created_at,
            updated_at,
            null::bigint as reporter_cid,
            null::text as reporter_name,
            null::bigint as reportee_cid,
            null::text as reportee_name
        "#,
    )
    .bind(id)
    .bind(reporter_id)
    .bind(reportee_id)
    .bind(timestamp)
    .bind(reason)
    .bind(reporter_callsign)
    .bind(reportee_callsign)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::BadRequest)
}

pub async fn count_my_incidents(
    pool: &PgPool,
    user_id: &str,
    closed: Option<bool>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.incident_reports
        where (reporter_id = $1 or reportee_id = $1)
          and ($2::bool is null or closed = $2)
        "#,
    )
    .bind(user_id)
    .bind(closed)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_my_incidents(
    pool: &PgPool,
    user_id: &str,
    closed: Option<bool>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<IncidentItem>, ApiError> {
    sqlx::query_as::<_, IncidentItemRow>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where (i.reporter_id = $1 or i.reportee_id = $1)
          and ($2::bool is null or i.closed = $2)
        order by i.timestamp desc, i.created_at desc, i.id asc
        limit $3 offset $4
        "#,
    )
    .bind(user_id)
    .bind(closed)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn count_all_incidents(pool: &PgPool, closed: Option<bool>) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from feedback.incident_reports where ($1::bool is null or closed = $1)",
    )
    .bind(closed)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_all_incidents(
    pool: &PgPool,
    closed: Option<bool>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<IncidentItem>, ApiError> {
    sqlx::query_as::<_, IncidentItemRow>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where ($1::bool is null or i.closed = $1)
        order by i.timestamp desc, i.created_at desc, i.id asc
        limit $2 offset $3
        "#,
    )
    .bind(closed)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_incident(pool: &PgPool, incident_id: &str) -> Result<IncidentItem, ApiError> {
    sqlx::query_as::<_, IncidentItemRow>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where i.id = $1
        "#,
    )
    .bind(incident_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .map(Into::into)
    .ok_or(ApiError::NotFound)
}

pub async fn update_incident_closed(
    pool: &PgPool,
    incident_id: &str,
    closed: bool,
) -> Result<Option<IncidentItem>, ApiError> {
    sqlx::query_as::<_, IncidentItemRow>(
        r#"
        update feedback.incident_reports i
        set closed = $2,
            updated_at = now()
        from identity.users ru, identity.users tu
        where i.id = $1
          and ru.id = i.reporter_id
          and tu.id = i.reportee_id
        returning
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        "#,
    )
    .bind(incident_id)
    .bind(closed)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}
