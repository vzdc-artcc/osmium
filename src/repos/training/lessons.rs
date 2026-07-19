use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::TrainingLesson};

const LESSON_COLUMNS: &str = r#"
    id,
    identifier,
    location,
    name,
    description,
    position,
    facility,
    rubric_id,
    updated_at,
    instructor_only,
    notify_instructor_on_pass,
    release_request_on_pass,
    duration,
    trainee_preparation,
    performance_indicator_template_id,
    created_at
"#;

pub async fn count_lessons<'e, E>(executor: E) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.lessons")
        .fetch_one(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_lessons<'e, E>(
    executor: E,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingLesson>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingLesson>(&format!(
        r#"
        select {LESSON_COLUMNS}
        from training.lessons
        order by location asc, identifier asc, name asc, id asc
        limit $1 offset $2
        "#
    ))
    .bind(page_size)
    .bind(offset)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_lesson<'e, E>(
    executor: E,
    id: &str,
    identifier: &str,
    location: i32,
    name: &str,
    description: &str,
    position: &str,
    facility: &str,
    now: DateTime<Utc>,
    instructor_only: bool,
    notify_instructor_on_pass: bool,
    release_request_on_pass: bool,
    duration: i32,
    trainee_preparation: Option<&str>,
    performance_indicator_template_id: Option<&str>,
) -> Result<TrainingLesson, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingLesson>(&format!(
        r#"
        insert into training.lessons (
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $8
        )
        returning {LESSON_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(identifier)
    .bind(location)
    .bind(name)
    .bind(description)
    .bind(position)
    .bind(facility)
    .bind(now)
    .bind(instructor_only)
    .bind(notify_instructor_on_pass)
    .bind(release_request_on_pass)
    .bind(duration)
    .bind(trainee_preparation)
    .bind(performance_indicator_template_id)
    .fetch_one(executor)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_lesson<'e, E>(
    executor: E,
    lesson_id: &str,
) -> Result<Option<TrainingLesson>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingLesson>(&format!(
        "select {LESSON_COLUMNS} from training.lessons where id = $1"
    ))
    .bind(lesson_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_lesson_row<'e, E>(
    executor: E,
    lesson_id: &str,
    identifier: &str,
    location: i32,
    name: &str,
    description: &str,
    position: &str,
    facility: &str,
    now: DateTime<Utc>,
    instructor_only: bool,
    notify_instructor_on_pass: bool,
    release_request_on_pass: bool,
    duration: i32,
    trainee_preparation: Option<&str>,
    performance_indicator_template_id: Option<&str>,
) -> Result<Option<TrainingLesson>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingLesson>(&format!(
        r#"
        update training.lessons
        set
            identifier = $2,
            location = $3,
            name = $4,
            description = $5,
            position = $6,
            facility = $7,
            updated_at = $8,
            instructor_only = $9,
            notify_instructor_on_pass = $10,
            release_request_on_pass = $11,
            duration = $12,
            trainee_preparation = $13,
            performance_indicator_template_id = $14
        where id = $1
        returning {LESSON_COLUMNS}
        "#
    ))
    .bind(lesson_id)
    .bind(identifier)
    .bind(location)
    .bind(name)
    .bind(description)
    .bind(position)
    .bind(facility)
    .bind(now)
    .bind(instructor_only)
    .bind(notify_instructor_on_pass)
    .bind(release_request_on_pass)
    .bind(duration)
    .bind(trainee_preparation)
    .bind(performance_indicator_template_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_lesson_row<'e, E>(
    executor: E,
    lesson_id: &str,
) -> Result<Option<TrainingLesson>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, TrainingLesson>(&format!(
        r#"
        delete from training.lessons
        where id = $1
        returning {LESSON_COLUMNS}
        "#
    ))
    .bind(lesson_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::BadRequest)
}
