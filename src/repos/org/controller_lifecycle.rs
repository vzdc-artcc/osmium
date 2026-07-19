use sqlx::{Executor, PgPool, Postgres};

use crate::errors::ApiError;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct MembershipLifecycleRow {
    pub user_id: String,
    pub cid: i64,
    pub controller_status: String,
    pub artcc: String,
    pub operating_initials: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub show_welcome_message: bool,
}

pub async fn fetch_membership_lifecycle_row(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<MembershipLifecycleRow>, ApiError> {
    sqlx::query_as::<_, MembershipLifecycleRow>(
        r#"
        select
            m.user_id,
            u.cid,
            m.controller_status,
            m.artcc,
            m.operating_initials,
            u.first_name,
            u.last_name,
            u.display_name,
            p.show_welcome_message
        from org.memberships m
        join identity.users u on u.id = m.user_id
        join identity.user_profiles p on p.user_id = u.id
        where u.cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_membership_status<'e, E>(
    executor: E,
    user_id: &str,
    controller_status: &str,
    artcc: Option<&str>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update org.memberships
        set controller_status = $2,
            artcc = coalesce($3, artcc),
            updated_at = now()
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(controller_status)
    .bind(artcc)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn clear_operating_initials<'e, E>(executor: E, user_id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        r#"
        update org.memberships
        set operating_initials = null,
            updated_at = now()
        where user_id = $1
          and operating_initials is not null
        "#,
    )
    .bind(user_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_training_assignment_requests_for_user<'e, E>(
    executor: E,
    user_id: &str,
) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result =
        sqlx::query("delete from training.training_assignment_requests where student_id = $1")
            .bind(user_id)
            .execute(executor)
            .await
            .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}

pub async fn delete_training_assignments_for_user<'e, E>(
    executor: E,
    user_id: &str,
) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query("delete from training.training_assignments where student_id = $1")
        .bind(user_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}

pub async fn enable_welcome_message<'e, E>(executor: E, user_id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        "update identity.user_profiles set show_welcome_message = true, updated_at = now() where user_id = $1",
    )
    .bind(user_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() > 0)
}

pub async fn disable_welcome_message<'e, E>(executor: E, user_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "update identity.user_profiles set show_welcome_message = false, updated_at = now() where user_id = $1",
    )
    .bind(user_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}
