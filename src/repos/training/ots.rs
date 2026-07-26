use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::OtsRecommendationSummary};

#[derive(Debug, sqlx::FromRow)]
pub struct OtsRecommendationRow {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub assigned_instructor_id: Option<String>,
    pub assigned_instructor_cid: Option<i64>,
    pub assigned_instructor_name: Option<String>,
    pub notes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<OtsRecommendationRow> for OtsRecommendationSummary {
    fn from(row: OtsRecommendationRow) -> Self {
        OtsRecommendationSummary {
            id: row.id,
            student_id: row.student_id,
            student_cid: row.student_cid,
            student_name: row.student_name,
            assigned_instructor_id: row.assigned_instructor_id,
            assigned_instructor_cid: row.assigned_instructor_cid,
            assigned_instructor_name: row.assigned_instructor_name,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const OTS_SELECT: &str = r#"
    select
        o.id,
        o.student_id,
        s.cid as student_cid,
        s.display_name as student_name,
        o.assigned_instructor_id,
        i.cid as assigned_instructor_cid,
        i.display_name as assigned_instructor_name,
        o.notes,
        o.created_at,
        o.updated_at
    from training.ots_recommendations o
    join identity.users s on s.id = o.student_id
    left join identity.users i on i.id = o.assigned_instructor_id
"#;

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
    let sql = format!("{OTS_SELECT} order by o.created_at desc, o.id asc limit $1 offset $2");
    sqlx::query_as::<_, OtsRecommendationRow>(&sql)
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
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
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
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(notes)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn fetch_ots_recommendation<'e, E>(
    executor: E,
    recommendation_id: &str,
) -> Result<Option<OtsRecommendationSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!("{OTS_SELECT} where o.id = $1");
    sqlx::query_as::<_, OtsRecommendationRow>(&sql)
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
) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        r#"
        update training.ots_recommendations
        set assigned_instructor_id = $1
        where id = $2
        "#,
    )
    .bind(assigned_instructor_id)
    .bind(recommendation_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_ots_recommendation_row<'e, E>(
    executor: E,
    recommendation_id: &str,
) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query("delete from training.ots_recommendations where id = $1")
        .bind(recommendation_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::BadRequest)?;

    Ok(result.rows_affected() > 0)
}
