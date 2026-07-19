use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::OtsRecommendationSummary};

#[derive(Debug, sqlx::FromRow)]
pub struct OtsRecommendationRow {
    pub id: String,
    pub student_id: String,
    pub assigned_instructor_id: Option<String>,
    pub notes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<OtsRecommendationRow> for OtsRecommendationSummary {
    fn from(row: OtsRecommendationRow) -> Self {
        OtsRecommendationSummary {
            id: row.id,
            student_id: row.student_id,
            assigned_instructor_id: row.assigned_instructor_id,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn count_ots_recommendations<'e, E>(executor: E) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.ots_recommendations")
        .fetch_one(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_ots_recommendations<'e, E>(
    executor: E,
    page_size: i64,
    offset: i64,
) -> Result<Vec<OtsRecommendationSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, OtsRecommendationRow>(
        r#"
        select id, student_id, assigned_instructor_id, notes, created_at, updated_at
        from training.ots_recommendations
        order by created_at desc, id asc
        limit $1 offset $2
        "#,
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(executor)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn user_exists<'e, E>(executor: E, user_id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let found =
        sqlx::query_scalar::<_, String>("select id from identity.users where id = $1 limit 1")
            .bind(user_id)
            .fetch_optional(executor)
            .await
            .map_err(|_| ApiError::Internal)?;
    Ok(found.is_some())
}

pub async fn student_has_ots_recommendation<'e, E>(
    executor: E,
    student_id: &str,
) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let found = sqlx::query_scalar::<_, String>(
        "select id from training.ots_recommendations where student_id = $1 limit 1",
    )
    .bind(student_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(found.is_some())
}

pub async fn insert_ots_recommendation<'e, E>(
    executor: E,
    id: &str,
    student_id: &str,
    notes: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<OtsRecommendationSummary, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, OtsRecommendationRow>(
        r#"
        insert into training.ots_recommendations (
            id,
            student_id,
            assigned_instructor_id,
            notes,
            created_at,
            updated_at
        )
        values ($1, $2, null, $3, $4, $4)
        returning id, student_id, assigned_instructor_id, notes, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(notes)
    .bind(now)
    .fetch_one(executor)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_ots_recommendation<'e, E>(
    executor: E,
    recommendation_id: &str,
) -> Result<Option<OtsRecommendationSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, OtsRecommendationRow>(
        r#"
        select id, student_id, assigned_instructor_id, notes, created_at, updated_at
        from training.ots_recommendations
        where id = $1
        "#,
    )
    .bind(recommendation_id)
    .fetch_optional(executor)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn update_ots_recommendation_row<'e, E>(
    executor: E,
    recommendation_id: &str,
    assigned_instructor_id: Option<&str>,
) -> Result<Option<OtsRecommendationSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, OtsRecommendationRow>(
        r#"
        update training.ots_recommendations
        set assigned_instructor_id = $1
        where id = $2
        returning id, student_id, assigned_instructor_id, notes, created_at, updated_at
        "#,
    )
    .bind(assigned_instructor_id)
    .bind(recommendation_id)
    .fetch_optional(executor)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_ots_recommendation_row<'e, E>(
    executor: E,
    recommendation_id: &str,
) -> Result<Option<OtsRecommendationSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, OtsRecommendationRow>(
        r#"
        delete from training.ots_recommendations
        where id = $1
        returning id, student_id, assigned_instructor_id, notes, created_at, updated_at
        "#,
    )
    .bind(recommendation_id)
    .fetch_optional(executor)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::BadRequest)
}
