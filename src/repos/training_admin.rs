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

/// The progression currently assigned to a user, as `(id, name,
/// next_progression_id)`, or `None` if unassigned.
pub async fn get_user_progression(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<(String, String, Option<String>)>, ApiError> {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        r#"
        select tp.id, tp.name, tp.next_progression_id
        from training.user_progressions up
        join training.training_progressions tp on tp.id = up.progression_id
        where up.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// The first progression flagged `auto_assign_new_home_obs`, as `(id, name,
/// next_progression_id)` — the starter progression given to new home OBS in
/// roster sync. `None` if no progression is so flagged.
pub async fn find_auto_assign_home_obs_progression(
    pool: &PgPool,
) -> Result<Option<(String, String, Option<String>)>, ApiError> {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        r#"
        select id, name, next_progression_id
        from training.training_progressions
        where auto_assign_new_home_obs
        order by created_at asc
        limit 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Whether the controller has already received their one-time auto-assigned
/// starter progression (roster sync sets this the first time it assigns a new
/// home OBS a progression, so it never re-assigns after the user completes it).
pub async fn get_auto_assign_single_pass(
    pool: &PgPool,
    user_id: &str,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, Option<bool>>(
        "select flag_auto_assign_single_pass from identity.user_flags where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.flatten().unwrap_or(false))
    .map_err(|_| ApiError::Internal)
}

/// Marks the one-time starter-progression auto-assignment as done for a user.
pub async fn set_auto_assign_single_pass(pool: &PgPool, user_id: &str) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into identity.user_flags (user_id, flag_auto_assign_single_pass)
        values ($1, true)
        on conflict (user_id) do update
        set flag_auto_assign_single_pass = true,
            updated_at = now()
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct ProgressionStatusStepRow {
    pub step_id: String,
    pub lesson_id: String,
    pub lesson_identifier: String,
    pub lesson_name: String,
    pub sort_order: i32,
    pub optional: bool,
    pub passed: bool,
    pub training_session_id: Option<String>,
    pub session_end: Option<DateTime<Utc>>,
}

/// Per-step pass state for a progression, computed against a specific student.
/// For each step (ordered), the most recent training ticket for the step's
/// lesson where the session's student is `user_id` decides `passed`. Mirrors
/// the website's `getProgressionStatus`.
pub async fn compute_progression_status_steps(
    pool: &PgPool,
    progression_id: &str,
    user_id: &str,
) -> Result<Vec<ProgressionStatusStepRow>, ApiError> {
    sqlx::query_as::<_, ProgressionStatusStepRow>(
        r#"
        select
            s.id as step_id,
            s.lesson_id,
            l.identifier as lesson_identifier,
            l.name as lesson_name,
            s.sort_order,
            s.optional,
            coalesce(t.passed, false) as passed,
            t.session_id as training_session_id,
            t.session_end
        from training.training_progression_steps s
        join training.lessons l on l.id = s.lesson_id
        left join lateral (
            select tt.passed, ts.id as session_id, ts."end" as session_end
            from training.training_tickets tt
            join training.training_sessions ts on ts.id = tt.session_id
            where tt.lesson_id = s.lesson_id and ts.student_id = $2
            order by ts."end" desc
            limit 1
        ) t on true
        where s.progression_id = $1
        order by s.sort_order asc
        "#,
    )
    .bind(progression_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_dossier_entries(
    pool: &PgPool,
    cid: i64,
    include_confidential: bool,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.dossier_entries d
        join identity.users target on target.id = d.user_id
        where target.cid = $1
          and ($2 or not d.is_confidential)
        "#,
    )
    .bind(cid)
    .bind(include_confidential)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_dossier_entries(
    pool: &PgPool,
    cid: i64,
    include_confidential: bool,
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
            d.is_confidential,
            d.timestamp,
            d.created_at,
            u.cid as writer_cid,
            u.display_name as writer_name
        from feedback.dossier_entries d
        join identity.users target on target.id = d.user_id
        join identity.users u on u.id = d.writer_id
        where target.cid = $1
          and ($4 or not d.is_confidential)
        order by d.timestamp desc, d.created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(cid)
    .bind(page_size)
    .bind(offset)
    .bind(include_confidential)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_dossier_entry(
    pool: &PgPool,
    id: &str,
    user_id: &str,
    writer_id: &str,
    message: &str,
    is_confidential: bool,
) -> Result<DossierEntryItem, ApiError> {
    sqlx::query_as::<_, DossierEntryItem>(
        r#"
        with inserted as (
            insert into feedback.dossier_entries (id, user_id, writer_id, message, is_confidential, timestamp)
            values ($1, $2, $3, $4, $5, now())
            returning id, user_id, writer_id, message, is_confidential, timestamp, created_at
        )
        select
            inserted.id,
            inserted.user_id,
            inserted.writer_id,
            inserted.message,
            inserted.is_confidential,
            inserted.timestamp,
            inserted.created_at,
            u.cid as writer_cid,
            u.display_name as writer_name
        from inserted
        join identity.users u on u.id = inserted.writer_id
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(writer_id)
    .bind(message)
    .bind(is_confidential)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
