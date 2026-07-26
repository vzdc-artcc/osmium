use sqlx::PgPool;

use crate::{errors::ApiError, models::FeedbackItem};

#[derive(Debug, Default, Clone, Copy)]
pub struct FeedbackFilters<'a> {
    pub status: Option<&'a str>,
    pub submitter_cid: Option<i64>,
    pub submitter_name: Option<&'a str>,
    pub target_cid: Option<i64>,
    pub target_name: Option<&'a str>,
    pub controller_position: Option<&'a str>,
    pub min_rating: Option<i32>,
    pub max_rating: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
struct FeedbackItemRow {
    id: String,
    submitter_user_id: String,
    target_user_id: String,
    submitter_cid: Option<i64>,
    submitter_name: Option<String>,
    target_cid: Option<i64>,
    target_name: Option<String>,
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
            submitter_cid: row.submitter_cid,
            submitter_name: row.submitter_name,
            target_cid: row.target_cid,
            target_name: row.target_name,
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
            null::bigint as submitter_cid,
            null::text as submitter_name,
            null::bigint as target_cid,
            null::text as target_name,
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

pub async fn count_all(pool: &PgPool, filters: FeedbackFilters<'_>) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where ($1::text is null or f.status = $1)
          and ($2::bigint is null or su.cid = $2)
          and ($3::text is null or su.display_name ilike '%' || $3 || '%')
          and ($4::bigint is null or tu.cid = $4)
          and ($5::text is null or tu.display_name ilike '%' || $5 || '%')
          and ($6::text is null or f.controller_position ilike '%' || $6 || '%')
          and ($7::int4 is null or f.rating >= $7)
          and ($8::int4 is null or f.rating <= $8)
        "#,
    )
    .bind(filters.status)
    .bind(filters.submitter_cid)
    .bind(filters.submitter_name)
    .bind(filters.target_cid)
    .bind(filters.target_name)
    .bind(filters.controller_position)
    .bind(filters.min_rating)
    .bind(filters.max_rating)
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
            f.id,
            f.submitter_user_id,
            f.target_user_id,
            su.cid as submitter_cid,
            su.display_name as submitter_name,
            tu.cid as target_cid,
            tu.display_name as target_name,
            f.pilot_callsign,
            f.controller_position,
            f.rating,
            f.comments,
            f.staff_comments,
            f.status,
            f.submitted_at,
            f.decided_at,
            f.decided_by
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where f.target_user_id = $1
          and ($2::text is null or f.status = $2)
        order by f.submitted_at desc, f.id asc
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
    filters: FeedbackFilters<'_>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where f.submitter_user_id = $1
          and ($2::text is null or f.status = $2)
          and ($3::bigint is null or su.cid = $3)
          and ($4::text is null or su.display_name ilike '%' || $4 || '%')
          and ($5::bigint is null or tu.cid = $5)
          and ($6::text is null or tu.display_name ilike '%' || $6 || '%')
          and ($7::text is null or f.controller_position ilike '%' || $7 || '%')
          and ($8::int4 is null or f.rating >= $8)
          and ($9::int4 is null or f.rating <= $9)
        "#,
    )
    .bind(submitter_user_id)
    .bind(filters.status)
    .bind(filters.submitter_cid)
    .bind(filters.submitter_name)
    .bind(filters.target_cid)
    .bind(filters.target_name)
    .bind(filters.controller_position)
    .bind(filters.min_rating)
    .bind(filters.max_rating)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_all(
    pool: &PgPool,
    filters: FeedbackFilters<'_>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            f.id,
            f.submitter_user_id,
            f.target_user_id,
            su.cid as submitter_cid,
            su.display_name as submitter_name,
            tu.cid as target_cid,
            tu.display_name as target_name,
            f.pilot_callsign,
            f.controller_position,
            f.rating,
            f.comments,
            f.staff_comments,
            f.status,
            f.submitted_at,
            f.decided_at,
            f.decided_by
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where ($1::text is null or f.status = $1)
          and ($2::bigint is null or su.cid = $2)
          and ($3::text is null or su.display_name ilike '%' || $3 || '%')
          and ($4::bigint is null or tu.cid = $4)
          and ($5::text is null or tu.display_name ilike '%' || $5 || '%')
          and ($6::text is null or f.controller_position ilike '%' || $6 || '%')
          and ($7::int4 is null or f.rating >= $7)
          and ($8::int4 is null or f.rating <= $8)
        order by f.submitted_at desc, f.id asc
        limit $9 offset $10
        "#,
    )
    .bind(filters.status)
    .bind(filters.submitter_cid)
    .bind(filters.submitter_name)
    .bind(filters.target_cid)
    .bind(filters.target_name)
    .bind(filters.controller_position)
    .bind(filters.min_rating)
    .bind(filters.max_rating)
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
    filters: FeedbackFilters<'_>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FeedbackItem>, ApiError> {
    sqlx::query_as::<_, FeedbackItemRow>(
        r#"
        select
            f.id,
            f.submitter_user_id,
            f.target_user_id,
            su.cid as submitter_cid,
            su.display_name as submitter_name,
            tu.cid as target_cid,
            tu.display_name as target_name,
            f.pilot_callsign,
            f.controller_position,
            f.rating,
            f.comments,
            f.staff_comments,
            f.status,
            f.submitted_at,
            f.decided_at,
            f.decided_by
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where f.submitter_user_id = $1
          and ($2::text is null or f.status = $2)
          and ($3::bigint is null or su.cid = $3)
          and ($4::text is null or su.display_name ilike '%' || $4 || '%')
          and ($5::bigint is null or tu.cid = $5)
          and ($6::text is null or tu.display_name ilike '%' || $6 || '%')
          and ($7::text is null or f.controller_position ilike '%' || $7 || '%')
          and ($8::int4 is null or f.rating >= $8)
          and ($9::int4 is null or f.rating <= $9)
        order by f.submitted_at desc, f.id asc
        limit $10 offset $11
        "#,
    )
    .bind(submitter_user_id)
    .bind(filters.status)
    .bind(filters.submitter_cid)
    .bind(filters.submitter_name)
    .bind(filters.target_cid)
    .bind(filters.target_name)
    .bind(filters.controller_position)
    .bind(filters.min_rating)
    .bind(filters.max_rating)
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
            f.id,
            f.submitter_user_id,
            f.target_user_id,
            su.cid as submitter_cid,
            su.display_name as submitter_name,
            tu.cid as target_cid,
            tu.display_name as target_name,
            f.pilot_callsign,
            f.controller_position,
            f.rating,
            f.comments,
            f.staff_comments,
            f.status,
            f.submitted_at,
            f.decided_at,
            f.decided_by
        from feedback.feedback_items f
        join identity.users su on su.id = f.submitter_user_id
        join identity.users tu on tu.id = f.target_user_id
        where f.id = $1
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
        update feedback.feedback_items f
        set status = $1,
            staff_comments = $2,
            decided_at = $3,
            decided_by = $4
        from identity.users su, identity.users tu
        where f.id = $5
          and su.id = f.submitter_user_id
          and tu.id = f.target_user_id
        returning
            f.id,
            f.submitter_user_id,
            f.target_user_id,
            su.cid as submitter_cid,
            su.display_name as submitter_name,
            tu.cid as target_cid,
            tu.display_name as target_name,
            f.pilot_callsign,
            f.controller_position,
            f.rating,
            f.comments,
            f.staff_comments,
            f.status,
            f.submitted_at,
            f.decided_at,
            f.decided_by
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
