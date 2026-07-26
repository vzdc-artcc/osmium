use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{errors::ApiError, models::incidents::IncidentItem};

#[derive(Debug, Default, Clone, Copy)]
pub struct IncidentFilters<'a> {
    pub closed: Option<bool>,
    pub reporter_cid: Option<i64>,
    pub reporter_name: Option<&'a str>,
    pub reportee_cid: Option<i64>,
    pub reportee_name: Option<&'a str>,
}

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
    filters: IncidentFilters<'_>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where (i.reporter_id = $1 or i.reportee_id = $1)
          and ($2::bool is null or i.closed = $2)
          and ($3::bigint is null or ru.cid = $3)
          and ($4::text is null or ru.display_name ilike '%' || $4 || '%')
          and ($5::bigint is null or tu.cid = $5)
          and ($6::text is null or tu.display_name ilike '%' || $6 || '%')
        "#,
    )
    .bind(user_id)
    .bind(filters.closed)
    .bind(filters.reporter_cid)
    .bind(filters.reporter_name)
    .bind(filters.reportee_cid)
    .bind(filters.reportee_name)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_my_incidents(
    pool: &PgPool,
    user_id: &str,
    filters: IncidentFilters<'_>,
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
          and ($3::bigint is null or ru.cid = $3)
          and ($4::text is null or ru.display_name ilike '%' || $4 || '%')
          and ($5::bigint is null or tu.cid = $5)
          and ($6::text is null or tu.display_name ilike '%' || $6 || '%')
        order by i.timestamp desc, i.created_at desc, i.id asc
        limit $7 offset $8
        "#,
    )
    .bind(user_id)
    .bind(filters.closed)
    .bind(filters.reporter_cid)
    .bind(filters.reporter_name)
    .bind(filters.reportee_cid)
    .bind(filters.reportee_name)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn count_all_incidents(
    pool: &PgPool,
    filters: IncidentFilters<'_>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where ($1::bool is null or i.closed = $1)
          and ($2::bigint is null or ru.cid = $2)
          and ($3::text is null or ru.display_name ilike '%' || $3 || '%')
          and ($4::bigint is null or tu.cid = $4)
          and ($5::text is null or tu.display_name ilike '%' || $5 || '%')
        "#,
    )
    .bind(filters.closed)
    .bind(filters.reporter_cid)
    .bind(filters.reporter_name)
    .bind(filters.reportee_cid)
    .bind(filters.reportee_name)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_all_incidents(
    pool: &PgPool,
    filters: IncidentFilters<'_>,
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
          and ($2::bigint is null or ru.cid = $2)
          and ($3::text is null or ru.display_name ilike '%' || $3 || '%')
          and ($4::bigint is null or tu.cid = $4)
          and ($5::text is null or tu.display_name ilike '%' || $5 || '%')
        order by i.timestamp desc, i.created_at desc, i.id asc
        limit $6 offset $7
        "#,
    )
    .bind(filters.closed)
    .bind(filters.reporter_cid)
    .bind(filters.reporter_name)
    .bind(filters.reportee_cid)
    .bind(filters.reportee_name)
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
