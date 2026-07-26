use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres, Transaction};

use crate::{errors::ApiError, models::TrainingAssignment};

const ASSIGNMENT_SELECT: &str = r#"
    select
        a.id,
        a.student_id,
        s.cid as student_cid,
        s.display_name as student_name,
        coalesce(sm.controller_status, 'NONE') as student_controller_status,
        a.primary_trainer_id,
        pt.cid as primary_trainer_cid,
        pt.display_name as primary_trainer_name,
        coalesce(array_agg(ot.trainer_id) filter (where ot.trainer_id is not null), array[]::text[]) as other_trainer_ids,
        coalesce(
            (
                select json_agg(json_build_object('id', otu.id, 'cid', otu.cid, 'name', otu.display_name))
                from training.training_assignment_other_trainers ot2
                join identity.users otu on otu.id = ot2.trainer_id
                where ot2.assignment_id = a.id
            ),
            '[]'::json
        ) as other_trainers,
        a.created_at,
        a.updated_at
    from training.training_assignments a
    join identity.users s on s.id = a.student_id
    left join org.memberships sm on sm.user_id = s.id
    join identity.users pt on pt.id = a.primary_trainer_id
    left join training.training_assignment_other_trainers ot on ot.assignment_id = a.id
"#;

#[derive(Debug, sqlx::FromRow)]
struct AssignmentRow {
    id: String,
    student_id: String,
    student_cid: i64,
    student_name: String,
    student_controller_status: String,
    primary_trainer_id: String,
    primary_trainer_cid: i64,
    primary_trainer_name: String,
    other_trainer_ids: Vec<String>,
    other_trainers: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AssignmentRow {
    fn into_model(self) -> Result<TrainingAssignment, ApiError> {
        let other_trainers =
            serde_json::from_value(self.other_trainers).map_err(|_| ApiError::Internal)?;

        Ok(TrainingAssignment {
            id: self.id,
            student_id: self.student_id,
            student_cid: self.student_cid,
            student_name: self.student_name,
            student_controller_status: self.student_controller_status,
            primary_trainer_id: self.primary_trainer_id,
            primary_trainer_cid: self.primary_trainer_cid,
            primary_trainer_name: self.primary_trainer_name,
            other_trainer_ids: self.other_trainer_ids,
            other_trainers,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

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
    let sql = format!(
        "{ASSIGNMENT_SELECT} group by a.id, s.cid, s.display_name, sm.controller_status, pt.cid, pt.display_name order by a.created_at desc, a.id asc limit $1 offset $2"
    );
    sqlx::query_as::<_, AssignmentRow>(&sql)
        .bind(page_size)
        .bind(offset)
        .fetch_all(executor)
        .await
        .map_err(|_| ApiError::Internal)?
        .into_iter()
        .map(AssignmentRow::into_model)
        .collect()
}

pub async fn fetch_assignment<'e, E>(
    executor: E,
    id: &str,
) -> Result<Option<TrainingAssignment>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{ASSIGNMENT_SELECT} where a.id = $1 group by a.id, s.cid, s.display_name, sm.controller_status, pt.cid, pt.display_name"
    );
    sqlx::query_as::<_, AssignmentRow>(&sql)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)?
        .map(AssignmentRow::into_model)
        .transpose()
}

pub async fn fetch_assignment_by_student<'e, E>(
    executor: E,
    student_id: &str,
) -> Result<Option<TrainingAssignment>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{ASSIGNMENT_SELECT} where a.student_id = $1 group by a.id, s.cid, s.display_name, sm.controller_status, pt.cid, pt.display_name"
    );
    sqlx::query_as::<_, AssignmentRow>(&sql)
        .bind(student_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)?
        .map(AssignmentRow::into_model)
        .transpose()
}

pub async fn insert_assignment<'e, E>(
    executor: E,
    id: &str,
    student_id: &str,
    primary_trainer_id: &str,
    created_by_actor_id: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.training_assignments
            (id, student_id, primary_trainer_id, created_by_actor_id, created_at, updated_at)
        values ($1, $2, $3, $4, $5, $5)
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(primary_trainer_id)
    .bind(created_by_actor_id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn update_assignment_primary_trainer<'e, E>(
    executor: E,
    id: &str,
    primary_trainer_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "update training.training_assignments set primary_trainer_id = $2, updated_at = $3 where id = $1",
    )
    .bind(id)
    .bind(primary_trainer_id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn delete_assignment_row<'e, E>(executor: E, id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query("delete from training.training_assignments where id = $1")
        .bind(id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
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

pub async fn replace_other_trainers(
    tx: &mut Transaction<'_, Postgres>,
    assignment_id: &str,
    trainer_ids: &[String],
) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_assignment_other_trainers where assignment_id = $1")
        .bind(assignment_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    for trainer_id in trainer_ids {
        insert_other_trainer(&mut **tx, assignment_id, trainer_id).await?;
    }

    Ok(())
}

pub async fn user_exists<'e, E>(executor: E, user_id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let found = sqlx::query_scalar::<_, String>("select id from identity.users where id = $1")
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(found.is_some())
}

pub async fn fetch_user_identities_by_ids<'e, E>(
    executor: E,
    user_ids: &[String],
) -> Result<Vec<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>("select id from identity.users where id = any($1)")
        .bind(user_ids)
        .fetch_all(executor)
        .await
        .map_err(|_| ApiError::Internal)
}
