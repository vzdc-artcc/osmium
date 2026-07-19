use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres};

use crate::{errors::ApiError, models::TrainingAssignmentRequest};

pub async fn count_assignment_requests(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from training.training_assignment_requests",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_assignment_requests(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingAssignmentRequest>, ApiError> {
    sqlx::query_as::<_, TrainingAssignmentRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.training_assignment_requests order by submitted_at desc, id asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_assignment_request(
    pool: &PgPool,
    id: &str,
    student_id: &str,
    now: DateTime<Utc>,
) -> Result<TrainingAssignmentRequest, ApiError> {
    sqlx::query_as::<_, TrainingAssignmentRequest>(
        r#"
        insert into training.training_assignment_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_assignment_request(
    pool: &PgPool,
    request_id: &str,
) -> Result<Option<TrainingAssignmentRequest>, ApiError> {
    sqlx::query_as::<_, TrainingAssignmentRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.training_assignment_requests where id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn decide_assignment_request_row(
    pool: &PgPool,
    request_id: &str,
    status: &str,
    now: DateTime<Utc>,
    decided_by: &str,
) -> Result<Option<TrainingAssignmentRequest>, ApiError> {
    sqlx::query_as::<_, TrainingAssignmentRequest>(
        r#"
        update training.training_assignment_requests
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
    .map_err(|_| ApiError::Internal)
}

pub async fn assignment_request_exists(pool: &PgPool, request_id: &str) -> Result<bool, ApiError> {
    let found = sqlx::query_scalar::<_, String>(
        "select id from training.training_assignment_requests where id = $1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(found.is_some())
}

pub async fn add_interested_trainer<'e, E>(
    executor: E,
    request_id: &str,
    trainer_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.training_assignment_request_interested_trainers (assignment_request_id, trainer_id)
        values ($1, $2)
        on conflict (assignment_request_id, trainer_id) do nothing
        "#,
    )
    .bind(request_id)
    .bind(trainer_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn remove_interested_trainer<'e, E>(
    executor: E,
    request_id: &str,
    trainer_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "delete from training.training_assignment_request_interested_trainers where assignment_request_id = $1 and trainer_id = $2",
    )
    .bind(request_id)
    .bind(trainer_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}
