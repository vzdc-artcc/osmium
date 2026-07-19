use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{errors::ApiError, models::TrainerReleaseRequest};

#[derive(Debug, sqlx::FromRow)]
pub struct TrainerReleaseRequestRow {
    pub id: String,
    pub student_id: String,
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
            submitted_at: row.submitted_at,
            status: row.status,
            decided_at: row.decided_at,
            decided_by: row.decided_by,
        }
    }
}

pub async fn count_release_requests(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.trainer_release_requests")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_release_requests(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainerReleaseRequest>, ApiError> {
    sqlx::query_as::<_, TrainerReleaseRequestRow>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.trainer_release_requests order by submitted_at desc, id asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_release_request(
    pool: &PgPool,
    id: &str,
    student_id: &str,
    now: DateTime<Utc>,
) -> Result<TrainerReleaseRequest, ApiError> {
    sqlx::query_as::<_, TrainerReleaseRequestRow>(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_release_request(
    pool: &PgPool,
    request_id: &str,
) -> Result<Option<TrainerReleaseRequest>, ApiError> {
    sqlx::query_as::<_, TrainerReleaseRequestRow>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.trainer_release_requests where id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn decide_release_request_row(
    pool: &PgPool,
    request_id: &str,
    status: &str,
    now: DateTime<Utc>,
    decided_by: &str,
) -> Result<Option<TrainerReleaseRequest>, ApiError> {
    sqlx::query_as::<_, TrainerReleaseRequestRow>(
        r#"
        update training.trainer_release_requests
        set status = $1, decided_at = $2, decided_by = $3
        where id = $4
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(status)
    .bind(now)
    .bind(decided_by)
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}
