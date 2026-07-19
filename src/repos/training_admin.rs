use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{
    errors::ApiError,
    models::training_admin::{
        DossierEntryItem, PerformanceIndicatorCategoryItem, PerformanceIndicatorCriteriaItem,
        PerformanceIndicatorTemplateItem, ProgressionAssignmentItem, TrainingProgressionItem,
        TrainingProgressionStepItem,
    },
};

#[derive(Debug, sqlx::FromRow)]
struct ProgressionAssignmentRow {
    user_id: String,
    progression_id: String,
    assigned_at: DateTime<Utc>,
    assigned_by_actor_id: Option<String>,
    cid: Option<i64>,
    display_name: Option<String>,
    progression_name: Option<String>,
}

impl From<ProgressionAssignmentRow> for ProgressionAssignmentItem {
    fn from(row: ProgressionAssignmentRow) -> Self {
        ProgressionAssignmentItem {
            user_id: row.user_id,
            progression_id: row.progression_id,
            assigned_at: row.assigned_at,
            assigned_by_actor_id: row.assigned_by_actor_id,
            cid: row.cid,
            display_name: row.display_name,
            progression_name: row.progression_name,
        }
    }
}

pub async fn count_progressions(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.training_progressions")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_progressions(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingProgressionItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionItem>(
        "select id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at from training.training_progressions order by name asc, id asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_progression(
    pool: &PgPool,
    id: &str,
    name: &str,
    next_progression_id: Option<&str>,
    auto_assign_new_home_obs: bool,
    auto_assign_new_visitor: bool,
) -> Result<TrainingProgressionItem, ApiError> {
    sqlx::query_as::<_, TrainingProgressionItem>(
        r#"
        insert into training.training_progressions (
            id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, now(), now())
        returning id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(next_progression_id)
    .bind(auto_assign_new_home_obs)
    .bind(auto_assign_new_visitor)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_progression(
    pool: &PgPool,
    progression_id: &str,
) -> Result<Option<TrainingProgressionItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionItem>("select id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at from training.training_progressions where id = $1")
        .bind(progression_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_progression_row(
    pool: &PgPool,
    progression_id: &str,
    name: Option<&str>,
    next_progression_id_set: bool,
    next_progression_id: Option<String>,
    auto_assign_new_home_obs: Option<bool>,
    auto_assign_new_visitor: Option<bool>,
) -> Result<Option<TrainingProgressionItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionItem>(
        r#"
        update training.training_progressions
        set name = coalesce($2, name),
            next_progression_id = case when $3::bool then $4 else next_progression_id end,
            auto_assign_new_home_obs = coalesce($5, auto_assign_new_home_obs),
            auto_assign_new_visitor = coalesce($6, auto_assign_new_visitor),
            updated_at = now()
        where id = $1
        returning id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        "#,
    )
    .bind(progression_id)
    .bind(name)
    .bind(next_progression_id_set)
    .bind(next_progression_id)
    .bind(auto_assign_new_home_obs)
    .bind(auto_assign_new_visitor)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_progression_row(pool: &PgPool, progression_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_progressions where id = $1")
        .bind(progression_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_progression_steps(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.training_progression_steps")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_progression_steps(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingProgressionStepItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionStepItem>(
        "select id, progression_id, lesson_id, sort_order, optional, created_at from training.training_progression_steps order by progression_id asc, sort_order asc, id asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_progression_step(
    pool: &PgPool,
    id: &str,
    progression_id: &str,
    lesson_id: &str,
    sort_order: i32,
    optional: bool,
) -> Result<TrainingProgressionStepItem, ApiError> {
    sqlx::query_as::<_, TrainingProgressionStepItem>(
        r#"
        insert into training.training_progression_steps (id, progression_id, lesson_id, sort_order, optional, created_at)
        values ($1, $2, $3, $4, $5, now())
        returning id, progression_id, lesson_id, sort_order, optional, created_at
        "#,
    )
    .bind(id)
    .bind(progression_id)
    .bind(lesson_id)
    .bind(sort_order)
    .bind(optional)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_progression_step(
    pool: &PgPool,
    step_id: &str,
) -> Result<Option<TrainingProgressionStepItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionStepItem>("select id, progression_id, lesson_id, sort_order, optional, created_at from training.training_progression_steps where id = $1")
        .bind(step_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_progression_step_row(
    pool: &PgPool,
    step_id: &str,
    lesson_id: Option<&str>,
    sort_order: Option<i32>,
    optional: Option<bool>,
) -> Result<Option<TrainingProgressionStepItem>, ApiError> {
    sqlx::query_as::<_, TrainingProgressionStepItem>(
        r#"
        update training.training_progression_steps
        set lesson_id = coalesce($2, lesson_id),
            sort_order = coalesce($3, sort_order),
            optional = coalesce($4, optional)
        where id = $1
        returning id, progression_id, lesson_id, sort_order, optional, created_at
        "#,
    )
    .bind(step_id)
    .bind(lesson_id)
    .bind(sort_order)
    .bind(optional)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_progression_step_row(pool: &PgPool, step_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_progression_steps where id = $1")
        .bind(step_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_pi_templates(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from training.performance_indicator_templates",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_pi_templates(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<PerformanceIndicatorTemplateItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "select id, name, created_at, updated_at from training.performance_indicator_templates order by name asc, id asc limit $1 offset $2",
    ).bind(page_size).bind(offset).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn insert_pi_template(
    pool: &PgPool,
    id: &str,
    name: &str,
) -> Result<PerformanceIndicatorTemplateItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "insert into training.performance_indicator_templates (id, name, created_at, updated_at) values ($1, $2, now(), now()) returning id, name, created_at, updated_at",
    ).bind(id).bind(name).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_pi_template(
    pool: &PgPool,
    template_id: &str,
) -> Result<Option<PerformanceIndicatorTemplateItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorTemplateItem>("select id, name, created_at, updated_at from training.performance_indicator_templates where id = $1")
        .bind(template_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_pi_template_row(
    pool: &PgPool,
    template_id: &str,
    name: &str,
) -> Result<Option<PerformanceIndicatorTemplateItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "update training.performance_indicator_templates set name = $2, updated_at = now() where id = $1 returning id, name, created_at, updated_at",
    ).bind(template_id).bind(name).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_pi_template_row(pool: &PgPool, template_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.performance_indicator_templates where id = $1")
        .bind(template_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_pi_categories(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from training.performance_indicator_template_categories",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_pi_categories(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<PerformanceIndicatorCategoryItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "select id, template_id, name, sort_order from training.performance_indicator_template_categories order by template_id asc, sort_order asc, id asc limit $1 offset $2",
    ).bind(page_size).bind(offset).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn insert_pi_category(
    pool: &PgPool,
    id: &str,
    template_id: &str,
    name: &str,
    sort_order: i32,
) -> Result<PerformanceIndicatorCategoryItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "insert into training.performance_indicator_template_categories (id, template_id, name, sort_order) values ($1, $2, $3, $4) returning id, template_id, name, sort_order",
    ).bind(id).bind(template_id).bind(name).bind(sort_order).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_pi_category(
    pool: &PgPool,
    category_id: &str,
) -> Result<Option<PerformanceIndicatorCategoryItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCategoryItem>("select id, template_id, name, sort_order from training.performance_indicator_template_categories where id = $1")
        .bind(category_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_pi_category_row(
    pool: &PgPool,
    category_id: &str,
    name: Option<&str>,
    sort_order: Option<i32>,
) -> Result<Option<PerformanceIndicatorCategoryItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "update training.performance_indicator_template_categories set name = coalesce($2, name), sort_order = coalesce($3, sort_order) where id = $1 returning id, template_id, name, sort_order",
    ).bind(category_id).bind(name).bind(sort_order).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_pi_category_row(pool: &PgPool, category_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.performance_indicator_template_categories where id = $1")
        .bind(category_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_pi_criteria(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from training.performance_indicator_template_criteria",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_pi_criteria(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<PerformanceIndicatorCriteriaItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "select id, category_id, name, sort_order from training.performance_indicator_template_criteria order by category_id asc, sort_order asc, id asc limit $1 offset $2",
    ).bind(page_size).bind(offset).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn insert_pi_criteria(
    pool: &PgPool,
    id: &str,
    category_id: &str,
    name: &str,
    sort_order: i32,
) -> Result<PerformanceIndicatorCriteriaItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "insert into training.performance_indicator_template_criteria (id, category_id, name, sort_order) values ($1, $2, $3, $4) returning id, category_id, name, sort_order",
    ).bind(id).bind(category_id).bind(name).bind(sort_order).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_pi_criteria(
    pool: &PgPool,
    criteria_id: &str,
) -> Result<Option<PerformanceIndicatorCriteriaItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>("select id, category_id, name, sort_order from training.performance_indicator_template_criteria where id = $1")
        .bind(criteria_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_pi_criteria_row(
    pool: &PgPool,
    criteria_id: &str,
    name: Option<&str>,
    sort_order: Option<i32>,
) -> Result<Option<PerformanceIndicatorCriteriaItem>, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "update training.performance_indicator_template_criteria set name = coalesce($2, name), sort_order = coalesce($3, sort_order) where id = $1 returning id, category_id, name, sort_order",
    ).bind(criteria_id).bind(name).bind(sort_order).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_pi_criteria_row(pool: &PgPool, criteria_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.performance_indicator_template_criteria where id = $1")
        .bind(criteria_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_progression_assignments(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from training.user_progressions")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_progression_assignments(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<ProgressionAssignmentItem>, ApiError> {
    sqlx::query_as::<_, ProgressionAssignmentRow>(
        r#"
        select
            up.user_id,
            up.progression_id,
            up.assigned_at,
            up.assigned_by_actor_id,
            u.cid,
            u.display_name,
            tp.name as progression_name
        from training.user_progressions up
        join identity.users u on u.id = up.user_id
        join training.training_progressions tp on tp.id = up.progression_id
        order by up.assigned_at desc, up.user_id asc
        limit $1 offset $2
        "#,
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn upsert_progression_assignment(
    pool: &PgPool,
    user_id: &str,
    progression_id: &str,
    assigned_by_actor_id: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.user_progressions (user_id, progression_id, assigned_at, assigned_by_actor_id)
        values ($1, $2, now(), $3)
        on conflict (user_id) do update
        set progression_id = excluded.progression_id,
            assigned_at = excluded.assigned_at,
            assigned_by_actor_id = excluded.assigned_by_actor_id
        "#,
    ).bind(user_id).bind(progression_id).bind(assigned_by_actor_id).execute(pool).await.map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn fetch_progression_assignment(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<ProgressionAssignmentItem>, ApiError> {
    sqlx::query_as::<_, ProgressionAssignmentRow>(
        r#"
        select
            up.user_id,
            up.progression_id,
            up.assigned_at,
            up.assigned_by_actor_id,
            u.cid,
            u.display_name,
            tp.name as progression_name
        from training.user_progressions up
        join identity.users u on u.id = up.user_id
        join training.training_progressions tp on tp.id = up.progression_id
        where up.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_progression_assignment_row(
    pool: &PgPool,
    user_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from training.user_progressions where user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_dossier_entries(pool: &PgPool, cid: i64) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.dossier_entries d
        join identity.users target on target.id = d.user_id
        where target.cid = $1
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_dossier_entries(
    pool: &PgPool,
    cid: i64,
    page_size: i64,
    offset: i64,
) -> Result<Vec<DossierEntryItem>, ApiError> {
    sqlx::query_as::<_, DossierEntryItem>(
        r#"
        select
            d.id,
            d.user_id,
            d.writer_id,
            d.message,
            d.timestamp,
            d.created_at,
            u.cid as writer_cid,
            u.display_name as writer_name
        from feedback.dossier_entries d
        join identity.users target on target.id = d.user_id
        join identity.users u on u.id = d.writer_id
        where target.cid = $1
        order by d.timestamp desc, d.created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(cid)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
