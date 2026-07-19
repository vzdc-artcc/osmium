use sqlx::PgPool;

use crate::{errors::ApiError, models::FeedbackItem};

#[derive(Debug, sqlx::FromRow)]
struct FeedbackItemRow {
    id: String,
    submitter_user_id: String,
    target_user_id: String,
    pilot_callsign: String,
    controller_position: String,
    rating: i32,
    comments: Option<String>,
    staff_comments: Option<String>,
    status: String,
    submitted_at: chrono::DateTime<chrono::Utc>,
    decided_at: Option<chrono::DateTime<chrono::Utc>>,
    decided_by: Option<String>,
}

impl From<FeedbackItemRow> for FeedbackItem {
    fn from(row: FeedbackItemRow) -> Self {
        FeedbackItem {
            id: row.id,
            submitter_user_id: row.submitter_user_id,
            target_user_id: row.target_user_id,
            pilot_callsign: row.pilot_callsign,
            controller_position: row.controller_position,
            rating: row.rating,
            comments: row.comments,
            staff_comments: row.staff_comments,
            status: row.status,
            submitted_at: row.submitted_at,
            decided_at: row.decided_at,
            decided_by: row.decided_by,
        }
    }
}

pub async fn find_user_id_by_cid(pool: &PgPool, cid: i64) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_feedback_item(
    pool: &PgPool,
    id: &str,
    submitter_user_id: &str,
    target_user_id: &str,
    pilot_callsign: &str,
    controller_position: &str,
    rating: i32,
    comments: Option<&str>,
    submitted_at: chrono::DateTime<chrono::Utc>,
) -> Result<FeedbackItem, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        insert into feedback.feedback_items (
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            status,
            submitted_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8)
        returning
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        "#,
    )
    .bind(id)
    .bind(submitter_user_id)
    .bind(target_user_id)
    .bind(pilot_callsign)
    .bind(controller_position)
    .bind(rating)
    .bind(comments)
    .bind(submitted_at)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::Internal)
}

pub async fn count_all(pool: &PgPool, status: Option<&str>) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.feedback_items
        where ($1::text is null or status = $1)
        "#,
    )
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_by_target(
    pool: &PgPool,
    target_user_id: &str,
    status: Option<&str>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.feedback_items
        where target_user_id = $1
          and ($2::text is null or status = $2)
        "#,
    )
    .bind(target_user_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_by_target(
    pool: &PgPool,
    target_user_id: &str,
    status: Option<&str>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        from feedback.feedback_items
        where target_user_id = $1
          and ($2::text is null or status = $2)
        order by submitted_at desc, id asc
        limit $3 offset $4
        "#,
    )
    .bind(target_user_id)
    .bind(status)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn count_by_submitter(
    pool: &PgPool,
    submitter_user_id: &str,
    status: Option<&str>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.feedback_items
        where submitter_user_id = $1
          and ($2::text is null or status = $2)
        "#,
    )
    .bind(submitter_user_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_all(
    pool: &PgPool,
    status: Option<&str>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        from feedback.feedback_items
        where ($1::text is null or status = $1)
        order by submitted_at desc, id asc
        limit $2 offset $3
        "#,
    )
    .bind(status)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn list_by_submitter(
    pool: &PgPool,
    submitter_user_id: &str,
    status: Option<&str>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        from feedback.feedback_items
        where submitter_user_id = $1
          and ($2::text is null or status = $2)
        order by submitted_at desc, id asc
        limit $3 offset $4
        "#,
    )
    .bind(submitter_user_id)
    .bind(status)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn find_by_id(
    pool: &PgPool,
    feedback_id: &str,
) -> Result<Option<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        from feedback.feedback_items
        where id = $1
        "#,
    )
    .bind(feedback_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn update_decision(
    pool: &PgPool,
    feedback_id: &str,
    status: &str,
    staff_comments: Option<&str>,
    decided_at: chrono::DateTime<chrono::Utc>,
    decided_by: &str,
) -> Result<Option<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        update feedback.feedback_items
        set status = $1,
            staff_comments = $2,
            decided_at = $3,
            decided_by = $4
        where id = $5
        returning
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        "#,
    )
    .bind(status)
    .bind(staff_comments)
    .bind(decided_at)
    .bind(decided_by)
    .bind(feedback_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}
