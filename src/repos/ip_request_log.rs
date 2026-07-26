//! Durable per-request IP metadata (spec 011).
//!
//! Writes go through a batched, off-hot-path insert ([`insert_batch`], driven by the
//! drain job); reads back a single user's history for the admin endpoint. Deliberately
//! kept separate from `access.audit_logs` — this is a narrow, high-volume table.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::errors::ApiError;

/// A single buffered request record, pushed onto the mpsc channel by the request
/// logging middleware and bulk-inserted by the drain job. `actor_ref` is the raw
/// identity/service-account id; the DB join in [`insert_batch`] resolves it to the
/// `access.actors` id off the hot path.
#[derive(Debug, Clone)]
pub struct IpRequestLogEntry {
    pub ip_address: String,
    pub method: String,
    pub matched_path: String,
    pub status_code: i16,
    pub actor_type: Option<String>,
    pub actor_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One row of a user's IP history, for the admin endpoint.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct IpRequestLogItem {
    pub ip_address: String,
    pub method: String,
    pub matched_path: String,
    pub status_code: i16,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
}

/// Paginated response body for `GET /admin/users/{cid}/ip-history`.
#[derive(Debug, Serialize, ToSchema)]
pub struct IpRequestLogListResponse {
    pub items: Vec<IpRequestLogItem>,
    pub pagination: crate::models::PaginationMeta,
}

/// Bulk-inserts buffered entries in a single statement. Resolves each entry's
/// `actor_id` (FK to `access.actors`) via a left join off the request hot path —
/// anonymous requests and unmatched actors land a null `actor_id`.
pub async fn insert_batch(pool: &PgPool, entries: &[IpRequestLogEntry]) -> Result<(), ApiError> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut ip_addresses = Vec::with_capacity(entries.len());
    let mut methods = Vec::with_capacity(entries.len());
    let mut matched_paths = Vec::with_capacity(entries.len());
    let mut status_codes = Vec::with_capacity(entries.len());
    let mut actor_types: Vec<Option<String>> = Vec::with_capacity(entries.len());
    let mut actor_refs: Vec<Option<String>> = Vec::with_capacity(entries.len());
    let mut created_ats = Vec::with_capacity(entries.len());

    for entry in entries {
        ip_addresses.push(entry.ip_address.clone());
        methods.push(entry.method.clone());
        matched_paths.push(entry.matched_path.clone());
        status_codes.push(entry.status_code);
        actor_types.push(entry.actor_type.clone());
        actor_refs.push(entry.actor_ref.clone());
        created_ats.push(entry.created_at);
    }

    sqlx::query(
        r#"
        insert into access.ip_request_log
            (ip_address, method, matched_path, status_code, actor_type, actor_id, created_at)
        select
            v.ip_address::inet,
            v.method,
            v.matched_path,
            v.status_code,
            v.actor_type,
            a.id,
            v.created_at
        from unnest($1::text[], $2::text[], $3::text[], $4::smallint[], $5::text[], $6::text[], $7::timestamptz[])
            as v(ip_address, method, matched_path, status_code, actor_type, actor_ref, created_at)
        left join access.actors a
          on (v.actor_type = 'user' and a.user_id = v.actor_ref)
          or (v.actor_type = 'service_account' and a.service_account_id = v.actor_ref)
        "#,
    )
    .bind(&ip_addresses)
    .bind(&methods)
    .bind(&matched_paths)
    .bind(&status_codes)
    .bind(&actor_types)
    .bind(&actor_refs)
    .bind(&created_ats)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

/// Deletes rows older than `cutoff`; returns how many were removed. Used by the
/// retention cleanup job.
pub async fn delete_older_than(pool: &PgPool, cutoff: DateTime<Utc>) -> Result<u64, ApiError> {
    let result = sqlx::query("delete from access.ip_request_log where created_at < $1")
        .bind(cutoff)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}

/// A page of IP history for the actor belonging to `user_id`, newest first.
pub async fn list_for_user(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<IpRequestLogItem>, ApiError> {
    sqlx::query_as::<_, IpRequestLogItem>(
        r#"
        select
            host(l.ip_address) as ip_address,
            l.method,
            l.matched_path,
            l.status_code,
            l.created_at
        from access.ip_request_log l
        join access.actors a on a.id = l.actor_id
        where a.user_id = $1
        order by l.created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Count companion for [`list_for_user`].
pub async fn count_for_user(pool: &PgPool, user_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from access.ip_request_log l
        join access.actors a on a.id = l.actor_id
        where a.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
