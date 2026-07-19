use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::TrainingAssignment};

pub async fn count_assignments<'e, E>(executor: E) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.training_assignments")
        .fetch_one(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_assignments<'e, E>(
    executor: E,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingAssignment>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingAssignment>(
        "select id, student_id, primary_trainer_id, created_at, updated_at from training.training_assignments order by created_at desc, id asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_assignment<'e, E>(
    executor: E,
    id: &str,
    student_id: &str,
    primary_trainer_id: &str,
    now: DateTime<Utc>,
) -> Result<TrainingAssignment, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingAssignment>(
        r#"
        insert into training.training_assignments (id, student_id, primary_trainer_id, created_at, updated_at)
        values ($1, $2, $3, $4, $5)
        returning id, student_id, primary_trainer_id, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(primary_trainer_id)
    .bind(now)
    .bind(now)
    .fetch_one(executor)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn insert_other_trainer<'e, E>(
    executor: E,
    assignment_id: &str,
    trainer_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.training_assignment_other_trainers (assignment_id, trainer_id)
        values ($1, $2)
        on conflict (assignment_id, trainer_id) do nothing
        "#,
    )
    .bind(assignment_id)
    .bind(trainer_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}
