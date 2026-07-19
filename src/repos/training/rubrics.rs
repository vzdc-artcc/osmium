use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};

use crate::{
    errors::ApiError,
    models::{LessonRubricCellDetail, LessonRubricCriteriaDetail, LessonRubricDetail},
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CriteriaRow {
    pub id: String,
    pub rubric_id: String,
    pub criteria: String,
    pub description: String,
    pub passing: i32,
    pub max_points: i32,
}

pub async fn fetch_lesson_rubric_id<'e, E>(
    executor: E,
    lesson_id: &str,
) -> Result<Option<Option<String>>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, Option<String>>("select rubric_id from training.lessons where id = $1")
        .bind(lesson_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn insert_rubric<'e, E>(executor: E, id: &str, now: DateTime<Utc>) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "insert into training.lesson_rubrics (id, created_at, updated_at) values ($1, $2, $2)",
    )
    .bind(id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn set_lesson_rubric_id<'e, E>(
    executor: E,
    lesson_id: &str,
    rubric_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("update training.lessons set rubric_id = $2 where id = $1")
        .bind(lesson_id)
        .bind(rubric_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_criteria<'e, E>(
    executor: E,
    id: &str,
    rubric_id: &str,
    criteria: &str,
    description: &str,
    passing: i32,
    max_points: i32,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.lesson_rubric_criteria (
            id, rubric_id, criteria, description, passing, max_points, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $7)
        "#,
    )
    .bind(id)
    .bind(rubric_id)
    .bind(criteria)
    .bind(description)
    .bind(passing)
    .bind(max_points)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_criteria_for_lesson<'e, E>(
    executor: E,
    lesson_id: &str,
    criteria_id: &str,
) -> Result<Option<CriteriaRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, CriteriaRow>(
        r#"
        select c.id, c.rubric_id, c.criteria, c.description, c.passing, c.max_points
        from training.lesson_rubric_criteria c
        join training.lessons l on l.rubric_id = c.rubric_id
        where l.id = $1 and c.id = $2
        "#,
    )
    .bind(lesson_id)
    .bind(criteria_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_criteria_row<'e, E>(
    executor: E,
    criteria_id: &str,
    criteria: &str,
    description: &str,
    passing: i32,
    max_points: i32,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update training.lesson_rubric_criteria
        set criteria = $2, description = $3, passing = $4, max_points = $5, updated_at = $6
        where id = $1
        "#,
    )
    .bind(criteria_id)
    .bind(criteria)
    .bind(description)
    .bind(passing)
    .bind(max_points)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_criteria_row<'e, E>(executor: E, criteria_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("delete from training.lesson_rubric_criteria where id = $1")
        .bind(criteria_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_criteria_cells<'e, E>(
    executor: E,
    criteria_id: &str,
) -> Result<Vec<LessonRubricCellDetail>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, LessonRubricCellDetail>(
        r#"
        select id, criteria_id, points, description
        from training.lesson_rubric_cells
        where criteria_id = $1
        order by points asc, id asc
        "#,
    )
    .bind(criteria_id)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_cells_with_points<'e, E>(
    executor: E,
    criteria_id: &str,
    points: i32,
    exclude_cell_id: Option<&str>,
) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from training.lesson_rubric_cells
        where criteria_id = $1 and points = $2 and ($3::text is null or id != $3)
        "#,
    )
    .bind(criteria_id)
    .bind(points)
    .bind(exclude_cell_id)
    .fetch_one(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_cell<'e, E>(
    executor: E,
    id: &str,
    criteria_id: &str,
    points: i32,
    description: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.lesson_rubric_cells (id, criteria_id, points, description, created_at)
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(criteria_id)
    .bind(points)
    .bind(description)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_cell_for_criteria<'e, E>(
    executor: E,
    criteria_id: &str,
    cell_id: &str,
) -> Result<Option<LessonRubricCellDetail>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, LessonRubricCellDetail>(
        r#"
        select id, criteria_id, points, description
        from training.lesson_rubric_cells
        where criteria_id = $1 and id = $2
        "#,
    )
    .bind(criteria_id)
    .bind(cell_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_cell_row<'e, E>(
    executor: E,
    cell_id: &str,
    points: i32,
    description: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "update training.lesson_rubric_cells set points = $2, description = $3 where id = $1",
    )
    .bind(cell_id)
    .bind(points)
    .bind(description)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_cell_row<'e, E>(executor: E, cell_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("delete from training.lesson_rubric_cells where id = $1")
        .bind(cell_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_lesson_rubric_detail<'e, E>(
    executor: E,
    lesson_id: &str,
) -> Result<Option<LessonRubricDetail>, ApiError>
where
    E: Executor<'e, Database = Postgres> + Copy,
{
    let Some(rubric_id) = sqlx::query_scalar::<_, Option<String>>(
        "select rubric_id from training.lessons where id = $1",
    )
    .bind(lesson_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)?
    .flatten() else {
        return Ok(None);
    };

    let criteria_rows = sqlx::query_as::<_, CriteriaRow>(
        r#"
        select id, rubric_id, criteria, description, passing, max_points
        from training.lesson_rubric_criteria
        where rubric_id = $1
        order by sort_order asc, id asc
        "#,
    )
    .bind(&rubric_id)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut criteria = Vec::with_capacity(criteria_rows.len());
    for row in criteria_rows {
        let cells = fetch_criteria_cells(executor, &row.id).await?;
        criteria.push(LessonRubricCriteriaDetail {
            id: row.id,
            rubric_id: row.rubric_id,
            criteria: row.criteria,
            description: row.description,
            passing: row.passing,
            max_points: row.max_points,
            cells,
        });
    }

    Ok(Some(LessonRubricDetail {
        id: rubric_id,
        lesson_id: lesson_id.to_string(),
        criteria,
    }))
}
