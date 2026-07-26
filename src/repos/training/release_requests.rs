use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::TrainerReleaseRequest};

const RELEASE_REQUEST_SELECT: &str = r#"
    select
        r.id,
        r.student_id,
        s.cid as student_cid,
        s.display_name as student_name,
        coalesce(sm.controller_status, 'NONE') as student_controller_status,
        r.submitted_at,
        r.status,
        r.decided_at,
        r.decided_by
    from training.trainer_release_requests r
    join identity.users s on s.id = r.student_id
    left join org.memberships sm on sm.user_id = s.id
"#;

#[derive(Debug, sqlx::FromRow)]
pub struct TrainerReleaseRequestRow {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub student_controller_status: String,
    pub submitted_at: DateTime<Utc>,
    pub status: String,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
}

impl From<TrainerReleaseRequestRow> for TrainerReleaseRequest {
    fn from(row: TrainerReleaseRequestRow) -> Self {
        TrainerReleaseRequest {
            id: row.id,
            student_id: row.student_id,
            student_cid: row.student_cid,
            student_name: row.student_name,
            student_controller_status: row.student_controller_status,
            submitted_at: row.submitted_at,
            status: row.status,
            decided_at: row.decided_at,
            decided_by: row.decided_by,
        }
    }
}

pub async fn count_release_requests<'e, E>(executor: E) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.trainer_release_requests")
        .fetch_one(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_release_requests<'e, E>(
    executor: E,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainerReleaseRequest>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{RELEASE_REQUEST_SELECT} order by r.submitted_at desc, r.id asc limit $1 offset $2"
    );
    sqlx::query_as::<_, TrainerReleaseRequestRow>(&sql)
        .bind(page_size)
        .bind(offset)
        .fetch_all(executor)
        .await
        .map(|rows| rows.into_iter().map(Into::into).collect())
        .map_err(|_| ApiError::Internal)
}

pub async fn insert_release_request<'e, E>(
    executor: E,
    id: &str,
    student_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn fetch_release_request<'e, E>(
    executor: E,
    request_id: &str,
) -> Result<Option<TrainerReleaseRequest>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!("{RELEASE_REQUEST_SELECT} where r.id = $1");
    sqlx::query_as::<_, TrainerReleaseRequestRow>(&sql)
        .bind(request_id)
        .fetch_optional(executor)
        .await
        .map(|row| row.map(Into::into))
        .map_err(|_| ApiError::Internal)
}

pub async fn decide_release_request_row<'e, E>(
    executor: E,
    request_id: &str,
    status: &str,
    now: DateTime<Utc>,
    decided_by: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update training.trainer_release_requests
        set status = $1, decided_at = $2, decided_by = $3
        where id = $4
        "#,
    )
    .bind(status)
    .bind(now)
    .bind(decided_by)
    .bind(request_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn delete_release_request_row<'e, E>(
    executor: E,
    request_id: &str,
) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query("delete from training.trainer_release_requests where id = $1")
        .bind(request_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
}
