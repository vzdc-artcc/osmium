use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{errors::ApiError, models::JobRunItem};

pub async fn fetch_latest_job_run(
    pool: &PgPool,
    job_name: &str,
) -> Result<Option<JobRunItem>, ApiError> {
    sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where job_name = $1
        order by started_at desc
        limit 1
        "#,
    )
    .bind(job_name)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_recent_job_runs(
    pool: &PgPool,
    job_name: &str,
) -> Result<Vec<JobRunItem>, ApiError> {
    sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where job_name = $1
        order by started_at desc
        limit 10
        "#,
    )
    .bind(job_name)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn create_job_run(pool: &PgPool, job_name: &str) -> Result<String, ApiError> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into platform.job_runs (id, job_name, started_at, status, created_at)
        values ($1, $2, now(), 'running', now())
        "#,
    )
    .bind(&id)
    .bind(job_name)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(id)
}

pub async fn finish_job_run_success(
    pool: &PgPool,
    run_id: &str,
    result_summary: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update platform.job_runs
        set finished_at = now(),
            status = 'succeeded',
            result_summary = $2
        where id = $1
        "#,
    )
    .bind(run_id)
    .bind(result_summary)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn finish_job_run_failure(
    pool: &PgPool,
    run_id: &str,
    error_text: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update platform.job_runs
        set finished_at = now(),
            status = 'failed',
            error_text = $2
        where id = $1
        "#,
    )
    .bind(run_id)
    .bind(error_text)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_job_run(pool: &PgPool, run_id: &str) -> Result<JobRunItem, ApiError> {
    sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where id = $1
        "#,
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn lock_events_near_start(
    pool: &PgPool,
    threshold: DateTime<Utc>,
) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        update events.events
        set positions_locked = true,
            updated_at = now()
        where manual_positions_open = false
          and positions_locked = false
          and starts_at <= $1
          and archived_at is null
        "#,
    )
    .bind(threshold)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}

pub async fn archive_ended_events(
    pool: &PgPool,
    threshold: DateTime<Utc>,
) -> Result<i64, ApiError> {
    let result = sqlx::query(
        r#"
        update events.events
        set archived_at = coalesce(archived_at, now()),
            hidden = true,
            positions_locked = true,
            manual_positions_open = false,
            banner_asset_id = null,
            status = 'ARCHIVED',
            updated_at = now()
        where ends_at <= $1
          and archived_at is null
        "#,
    )
    .bind(threshold)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}
