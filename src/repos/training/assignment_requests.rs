use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres};

use crate::{errors::ApiError, models::TrainingAssignmentRequest};

const ASSIGNMENT_REQUEST_SELECT: &str = r#"
    select
        r.id,
        r.student_id,
        s.cid as student_cid,
        s.display_name as student_name,
        coalesce(sm.controller_status, 'NONE') as student_controller_status,
        sm.rating as student_rating,
        r.submitted_at,
        r.status,
        r.decided_at,
        r.decided_by,
        coalesce(
            (
                select json_agg(json_build_object('id', iu.id, 'cid', iu.cid, 'name', iu.display_name))
                from training.training_assignment_request_interested_trainers it
                join identity.users iu on iu.id = it.trainer_id
                where it.assignment_request_id = r.id
            ),
            '[]'::json
        ) as interested_trainers,
        coalesce(
            (
                select json_agg(
                    json_build_object('lesson_identifier', l.identifier, 'passed', t.passed)
                    order by t.created_at
                )
                from training.training_tickets t
                join training.lessons l on l.id = t.lesson_id
                where t.session_id = (
                    select ts.id
                    from training.training_sessions ts
                    where ts.student_id = r.student_id
                    order by ts.start desc, ts.id desc
                    limit 1
                )
            ),
            '[]'::json
        ) as last_session_tickets
    from training.training_assignment_requests r
    join identity.users s on s.id = r.student_id
    left join org.memberships sm on sm.user_id = s.id
"#;

#[derive(Debug, sqlx::FromRow)]
struct AssignmentRequestRow {
    id: String,
    student_id: String,
    student_cid: i64,
    student_name: String,
    student_controller_status: String,
    student_rating: Option<String>,
    submitted_at: DateTime<Utc>,
    status: String,
    decided_at: Option<DateTime<Utc>>,
    decided_by: Option<String>,
    interested_trainers: serde_json::Value,
    last_session_tickets: serde_json::Value,
}

impl AssignmentRequestRow {
    fn into_model(self) -> Result<TrainingAssignmentRequest, ApiError> {
        let interested_trainers =
            serde_json::from_value(self.interested_trainers).map_err(|_| ApiError::Internal)?;
        let last_session_tickets =
            serde_json::from_value(self.last_session_tickets).map_err(|_| ApiError::Internal)?;

        Ok(TrainingAssignmentRequest {
            id: self.id,
            student_id: self.student_id,
            student_cid: self.student_cid,
            student_name: self.student_name,
            student_controller_status: self.student_controller_status,
            student_rating: self.student_rating,
            submitted_at: self.submitted_at,
            status: self.status,
            decided_at: self.decided_at,
            decided_by: self.decided_by,
            interested_trainers,
            last_session_tickets,
        })
    }
}

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
    let sql = format!(
        "{ASSIGNMENT_REQUEST_SELECT} order by r.submitted_at desc, r.id asc limit $1 offset $2"
    );
    sqlx::query_as::<_, AssignmentRequestRow>(&sql)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .into_iter()
        .map(AssignmentRequestRow::into_model)
        .collect()
}

pub async fn insert_assignment_request(
    pool: &PgPool,
    id: &str,
    student_id: &str,
    now: DateTime<Utc>,
) -> Result<TrainingAssignmentRequest, ApiError> {
    sqlx::query(
        r#"
        insert into training.training_assignment_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    fetch_assignment_request(pool, id)
        .await?
        .ok_or(ApiError::Internal)
}

pub async fn fetch_assignment_request(
    pool: &PgPool,
    request_id: &str,
) -> Result<Option<TrainingAssignmentRequest>, ApiError> {
    let sql = format!("{ASSIGNMENT_REQUEST_SELECT} where r.id = $1");
    sqlx::query_as::<_, AssignmentRequestRow>(&sql)
        .bind(request_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .map(AssignmentRequestRow::into_model)
        .transpose()
}

pub async fn decide_assignment_request_row(
    pool: &PgPool,
    request_id: &str,
    status: &str,
    now: DateTime<Utc>,
    decided_by: &str,
) -> Result<Option<TrainingAssignmentRequest>, ApiError> {
    sqlx::query(
        r#"
        update training.training_assignment_requests
        set status = $1, decided_at = $2, decided_by = $3
        where id = $4
        "#,
    )
    .bind(status)
    .bind(now)
    .bind(decided_by)
    .bind(request_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    fetch_assignment_request(pool, request_id).await
}

pub async fn delete_assignment_request_row(
    pool: &PgPool,
    request_id: &str,
) -> Result<bool, ApiError> {
    let result = sqlx::query("delete from training.training_assignment_requests where id = $1")
        .bind(request_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
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
