use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    errors::ApiError,
    models::{
        AdditionalTrainerDetail, LessonRosterChangeSummary, OtsRecommendationSummary,
        RubricScoreDetail, TrainerReleaseRequest, TrainingSessionDetail, TrainingSessionListItem,
        TrainingSessionPerformanceIndicatorCategoryDetail,
        TrainingSessionPerformanceIndicatorCriteriaDetail,
        TrainingSessionPerformanceIndicatorDetail, TrainingTicketDetail,
    },
};

#[derive(Debug, sqlx::FromRow)]
pub struct SessionDetailRow {
    pub id: String,
    pub student_id: String,
    pub instructor_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub vatusa_id: Option<String>,
    pub enable_markdown: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub instructor_cid: i64,
    pub instructor_name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct TicketRow {
    pub id: String,
    pub session_id: String,
    pub lesson_id: String,
    pub passed: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ScoreRow {
    pub id: String,
    pub training_ticket_id: String,
    pub criteria_id: String,
    pub cell_id: String,
    pub passed: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IndicatorRootRow {
    pub id: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IndicatorCategoryRow {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, sqlx::FromRow)]
pub struct IndicatorCriteriaRow {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub sort_order: i32,
    pub marker: Option<String>,
    pub comments: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LessonRow {
    pub id: String,
    pub identifier: String,
    pub instructor_only: bool,
    pub notify_instructor_on_pass: bool,
    pub release_request_on_pass: bool,
    pub performance_indicator_template_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RubricMembershipRow {
    pub lesson_id: String,
    pub criteria_id: String,
    pub cell_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExistingTicketRow {
    pub lesson_id: String,
    pub passed: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MembershipRow {
    pub controller_status: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserIdentityRow {
    pub id: String,
    pub cid: i64,
    pub full_name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionExistsRow {
    pub id: String,
    pub instructor_id: String,
}

pub async fn fetch_session_detail_row(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<SessionDetailRow>, ApiError> {
    sqlx::query_as::<_, SessionDetailRow>(
        r#"
        select
            ts.id,
            ts.student_id,
            ts.instructor_id,
            ts.start,
            ts."end" as "end",
            ts.additional_comments,
            ts.trainer_comments,
            ts.vatusa_id,
            ts.enable_markdown,
            ts.created_at,
            ts.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            iu.cid as instructor_cid,
            iu.full_name as instructor_name
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        where ts.id = $1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_ticket_rows(
    pool: &PgPool,
    session_id: &str,
) -> Result<Vec<TicketRow>, ApiError> {
    sqlx::query_as::<_, TicketRow>(
        r#"
        select id, session_id, lesson_id, passed, created_at
        from training.training_tickets
        where session_id = $1
        order by created_at asc, id asc
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_score_rows(pool: &PgPool, session_id: &str) -> Result<Vec<ScoreRow>, ApiError> {
    sqlx::query_as::<_, ScoreRow>(
        r#"
        select id, training_ticket_id, criteria_id, cell_id, passed
        from training.rubric_scores
        where training_ticket_id in (
            select id from training.training_tickets where session_id = $1
        )
        order by id asc
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_performance_indicator_root(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<IndicatorRootRow>, ApiError> {
    sqlx::query_as::<_, IndicatorRootRow>(
        "select id from training.session_performance_indicators where training_session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_performance_indicator_categories(
    pool: &PgPool,
    root_id: &str,
) -> Result<Vec<IndicatorCategoryRow>, ApiError> {
    sqlx::query_as::<_, IndicatorCategoryRow>(
        r#"
        select id, name, sort_order
        from training.session_performance_indicator_categories
        where session_performance_indicator_id = $1
        order by sort_order asc, id asc
        "#,
    )
    .bind(root_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_performance_indicator_criteria(
    pool: &PgPool,
    root_id: &str,
) -> Result<Vec<IndicatorCriteriaRow>, ApiError> {
    sqlx::query_as::<_, IndicatorCriteriaRow>(
        r#"
        select id, category_id, name, sort_order, marker, comments
        from training.session_performance_indicator_criteria
        where category_id in (
            select id
            from training.session_performance_indicator_categories
            where session_performance_indicator_id = $1
        )
        order by sort_order asc, id asc
        "#,
    )
    .bind(root_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn count_sessions(
    pool: &PgPool,
    student_id: Option<&str>,
    instructor_id: Option<&str>,
    filter_field: &str,
    filter_pattern: &str,
    filter_is_exact: bool,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(distinct ts.id)::bigint
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        left join training.training_tickets tt on tt.session_id = ts.id
        left join training.lessons l on l.id = tt.lesson_id
        where ($1::text is null or ts.student_id = $1)
          and ($2::text is null or ts.instructor_id = $2)
          and (
            $3::text = ''
            or (
                $3 = 'student'
                and (
                    ($5 and (cast(su.cid as text) = $4 or su.full_name = $4))
                    or
                    (not $5 and (cast(su.cid as text) ilike $4 or su.full_name ilike $4))
                )
            )
            or (
                $3 = 'instructor'
                and (
                    ($5 and (cast(iu.cid as text) = $4 or iu.full_name = $4))
                    or
                    (not $5 and (cast(iu.cid as text) ilike $4 or iu.full_name ilike $4))
                )
            )
            or (
                $3 = 'lessons'
                and (
                    ($5 and (l.identifier = $4 or l.name = $4))
                    or
                    (not $5 and (l.identifier ilike $4 or l.name ilike $4))
                )
            )
          )
        "#,
    )
    .bind(student_id)
    .bind(instructor_id)
    .bind(filter_field)
    .bind(filter_pattern)
    .bind(filter_is_exact)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn list_sessions(
    pool: &PgPool,
    student_id: Option<&str>,
    instructor_id: Option<&str>,
    filter_field: &str,
    filter_pattern: &str,
    filter_is_exact: bool,
    sort_column: &str,
    sort_direction: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingSessionListItem>, ApiError> {
    let sql = format!(
        r#"
        select
            ts.id,
            ts.student_id,
            ts.instructor_id,
            ts.start,
            ts."end" as "end",
            ts.additional_comments,
            ts.trainer_comments,
            ts.vatusa_id,
            ts.enable_markdown,
            ts.created_at,
            ts.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            iu.cid as instructor_cid,
            iu.full_name as instructor_name,
            count(tt.id)::bigint as ticket_count,
            (
                select count(*)::bigint
                from training.training_session_additional_trainers sat
                where sat.session_id = ts.id
            ) as additional_trainer_count
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        left join training.training_tickets tt on tt.session_id = ts.id
        left join training.lessons l on l.id = tt.lesson_id
        where ($1::text is null or ts.student_id = $1)
          and ($2::text is null or ts.instructor_id = $2)
          and (
            $3::text = ''
            or (
                $3 = 'student'
                and (
                    ($5 and (cast(su.cid as text) = $4 or su.full_name = $4))
                    or
                    (not $5 and (cast(su.cid as text) ilike $4 or su.full_name ilike $4))
                )
            )
            or (
                $3 = 'instructor'
                and (
                    ($5 and (cast(iu.cid as text) = $4 or iu.full_name = $4))
                    or
                    (not $5 and (cast(iu.cid as text) ilike $4 or iu.full_name ilike $4))
                )
            )
            or (
                $3 = 'lessons'
                and (
                    ($5 and (l.identifier = $4 or l.name = $4))
                    or
                    (not $5 and (l.identifier ilike $4 or l.name ilike $4))
                )
            )
          )
        group by
            ts.id, su.cid, su.full_name, iu.cid, iu.full_name
        order by {sort_column} {sort_direction}
        limit $6 offset $7
        "#
    );

    sqlx::query_as::<_, TrainingSessionListItem>(&sql)
        .bind(student_id)
        .bind(instructor_id)
        .bind(filter_field)
        .bind(filter_pattern)
        .bind(filter_is_exact)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn fetch_student_identity(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Option<UserIdentityRow>, ApiError> {
    sqlx::query_as::<_, UserIdentityRow>(
        "select id, cid, full_name from identity.users where id = $1",
    )
    .bind(student_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_identities_by_ids(
    tx: &mut Transaction<'_, Postgres>,
    user_ids: &[String],
) -> Result<Vec<UserIdentityRow>, ApiError> {
    sqlx::query_as::<_, UserIdentityRow>(
        "select id, cid, full_name from identity.users where id = any($1)",
    )
    .bind(user_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_lessons_by_ids(
    tx: &mut Transaction<'_, Postgres>,
    lesson_ids: &[String],
) -> Result<Vec<LessonRow>, ApiError> {
    sqlx::query_as::<_, LessonRow>(
        r#"
        select
            id,
            identifier,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            performance_indicator_template_id
        from training.lessons
        where id = any($1)
        "#,
    )
    .bind(lesson_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_rubric_membership_rows(
    tx: &mut Transaction<'_, Postgres>,
    lesson_ids: &[String],
) -> Result<Vec<RubricMembershipRow>, ApiError> {
    sqlx::query_as::<_, RubricMembershipRow>(
        r#"
        select
            l.id as lesson_id,
            c.id as criteria_id,
            cell.id as cell_id
        from training.lessons l
        join training.lesson_rubrics r on r.id = l.rubric_id
        join training.lesson_rubric_criteria c on c.rubric_id = r.id
        left join training.lesson_rubric_cells cell on cell.criteria_id = c.id
        where l.id = any($1)
        "#,
    )
    .bind(lesson_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_membership_row(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Option<MembershipRow>, ApiError> {
    sqlx::query_as::<_, MembershipRow>(
        "select controller_status from org.memberships where user_id = $1",
    )
    .bind(student_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_session_exists_row(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<Option<SessionExistsRow>, ApiError> {
    sqlx::query_as::<_, SessionExistsRow>(
        "select id, instructor_id from training.training_sessions where id = $1",
    )
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_old_tickets(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<Vec<ExistingTicketRow>, ApiError> {
    sqlx::query_as::<_, ExistingTicketRow>(
        r#"
        select lesson_id, passed
        from training.training_tickets
        where session_id = $1
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_session_performance_indicators(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "delete from training.session_performance_indicators where training_session_id = $1",
    )
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_session_tickets(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_tickets where session_id = $1")
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_session_additional_trainers(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_session_additional_trainers where session_id = $1")
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_session_additional_trainer_row(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
    trainer_id: &str,
    description: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.training_session_additional_trainers (session_id, trainer_id, description)
        values ($1, $2, $3)
        "#,
    )
    .bind(session_id)
    .bind(trainer_id)
    .bind(description)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_session_additional_trainers(
    pool: &PgPool,
    session_id: &str,
) -> Result<Vec<AdditionalTrainerDetail>, ApiError> {
    sqlx::query_as::<_, AdditionalTrainerDetail>(
        r#"
        select
            sat.trainer_id,
            u.cid as trainer_cid,
            u.full_name as trainer_name,
            sat.description
        from training.training_session_additional_trainers sat
        join identity.users u on u.id = sat.trainer_id
        where sat.session_id = $1
        order by u.full_name asc, sat.trainer_id asc
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_session_row(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
    student_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    additional_comments: Option<&str>,
    trainer_comments: Option<&str>,
    enable_markdown: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update training.training_sessions
        set student_id = $2,
            start = $3,
            "end" = $4,
            additional_comments = $5,
            trainer_comments = $6,
            enable_markdown = $7,
            updated_at = $8
        where id = $1
        "#,
    )
    .bind(session_id)
    .bind(student_id)
    .bind(start)
    .bind(end)
    .bind(additional_comments)
    .bind(trainer_comments)
    .bind(enable_markdown)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_session_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    student_id: &str,
    instructor_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    additional_comments: Option<&str>,
    trainer_comments: Option<&str>,
    enable_markdown: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.training_sessions (
            id,
            student_id,
            instructor_id,
            start,
            "end",
            additional_comments,
            trainer_comments,
            enable_markdown,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(instructor_id)
    .bind(start)
    .bind(end)
    .bind(additional_comments)
    .bind(trainer_comments)
    .bind(enable_markdown)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_ticket_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    session_id: &str,
    lesson_id: &str,
    passed: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.training_tickets (id, session_id, lesson_id, passed, created_at)
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(session_id)
    .bind(lesson_id)
    .bind(passed)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_rubric_score_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    training_ticket_id: &str,
    criteria_id: &str,
    cell_id: &str,
    passed: bool,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.rubric_scores (
            id,
            training_ticket_id,
            criteria_id,
            cell_id,
            passed
        )
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(training_ticket_id)
    .bind(criteria_id)
    .bind(cell_id)
    .bind(passed)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_performance_indicator_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    session_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.session_performance_indicators (id, training_session_id, created_at)
        values ($1, $2, $3)
        "#,
    )
    .bind(id)
    .bind(session_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_performance_indicator_category_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    indicator_id: &str,
    name: &str,
    sort_order: i32,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.session_performance_indicator_categories (
            id,
            session_performance_indicator_id,
            name,
            sort_order
        )
        values ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(indicator_id)
    .bind(name)
    .bind(sort_order)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_performance_indicator_criteria_row(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    category_id: &str,
    name: &str,
    sort_order: i32,
    marker: &str,
    comments: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.session_performance_indicator_criteria (
            id,
            category_id,
            name,
            sort_order,
            marker,
            comments
        )
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(category_id)
    .bind(name)
    .bind(sort_order)
    .bind(marker)
    .bind(comments)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_session_row(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<Option<SessionExistsRow>, ApiError> {
    sqlx::query_as::<_, SessionExistsRow>(
        r#"
        delete from training.training_sessions
        where id = $1
        returning id, instructor_id
        "#,
    )
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_lesson_roster_changes(
    tx: &mut Transaction<'_, Postgres>,
    lesson_ids: &[String],
) -> Result<Vec<LessonRosterChangeSummary>, ApiError> {
    sqlx::query_as::<_, LessonRosterChangeSummary>(
        r#"
        select
            id,
            lesson_id,
            certification_type_id,
            certification_option,
            dossier_text
        from training.lesson_roster_changes
        where lesson_id = any($1)
        "#,
    )
    .bind(lesson_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_solo_certification_for_roster(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    certification_type_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "delete from org.user_solo_certifications where user_id = $1 and certification_type_id = $2",
    )
    .bind(user_id)
    .bind(certification_type_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_user_certification(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    user_id: &str,
    certification_type_id: &str,
    certification_option: &str,
    granted_at: DateTime<Utc>,
    granted_by_actor_id: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into org.user_certifications (
            id,
            user_id,
            certification_type_id,
            certification_option,
            granted_at,
            granted_by_actor_id
        )
        values ($1, $2, $3, $4, $5, $6)
        on conflict (user_id, certification_type_id) do update
        set certification_option = excluded.certification_option,
            granted_by_actor_id = excluded.granted_by_actor_id
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(certification_type_id)
    .bind(certification_option)
    .bind(granted_at)
    .bind(granted_by_actor_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_dossier_entry(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    user_id: &str,
    writer_id: &str,
    message: &str,
    timestamp: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into feedback.dossier_entries (id, user_id, writer_id, message, timestamp, created_at)
        values ($1, $2, $3, $4, $5, $5)
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(writer_id)
    .bind(message)
    .bind(timestamp)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn fetch_assignment_for_student(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select id from training.training_assignments where student_id = $1",
    )
    .bind(student_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_existing_release_request_for_student(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select id from training.trainer_release_requests where student_id = $1",
    )
    .bind(student_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_release_request_from_session(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    student_id: &str,
    now: DateTime<Utc>,
) -> Result<TrainerReleaseRequest, ApiError> {
    sqlx::query_as::<_, super::release_requests::TrainerReleaseRequestRow>(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status, created_at, updated_at)
        values ($1, $2, $3, 'PENDING', $3, $3)
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_ots_recommendations_for_student(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "delete from training.ots_recommendations where student_id = $1 returning id",
    )
    .bind(student_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_ots_recommendation_for_student(
    tx: &mut Transaction<'_, Postgres>,
    student_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select id from training.ots_recommendations where student_id = $1 limit 1",
    )
    .bind(student_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_ots_recommendation_note(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    student_id: &str,
    note: &str,
    now: DateTime<Utc>,
) -> Result<OtsRecommendationSummary, ApiError> {
    sqlx::query_as::<_, super::ots::OtsRecommendationRow>(
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
    .bind(note)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_session_detail(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<TrainingSessionDetail>, ApiError> {
    let Some(session) = fetch_session_detail_row(pool, session_id).await? else {
        return Ok(None);
    };

    let ticket_rows = fetch_ticket_rows(pool, session_id).await?;
    let score_rows = fetch_score_rows(pool, session_id).await?;

    let mut scores_by_ticket: std::collections::HashMap<String, Vec<RubricScoreDetail>> =
        std::collections::HashMap::new();
    for row in score_rows {
        scores_by_ticket
            .entry(row.training_ticket_id)
            .or_default()
            .push(RubricScoreDetail {
                id: row.id,
                criteria_id: row.criteria_id,
                cell_id: row.cell_id,
                passed: row.passed,
            });
    }

    let tickets = ticket_rows
        .into_iter()
        .map(|row| TrainingTicketDetail {
            id: row.id.clone(),
            session_id: row.session_id,
            lesson_id: row.lesson_id,
            passed: row.passed,
            created_at: row.created_at,
            scores: scores_by_ticket.remove(&row.id).unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    let performance_indicator = fetch_performance_indicator(pool, session_id).await?;
    let additional_trainers = fetch_session_additional_trainers(pool, session_id).await?;

    Ok(Some(TrainingSessionDetail {
        id: session.id,
        student_id: session.student_id,
        instructor_id: session.instructor_id,
        start: session.start,
        end: session.end,
        additional_comments: session.additional_comments,
        trainer_comments: session.trainer_comments,
        vatusa_id: session.vatusa_id,
        enable_markdown: session.enable_markdown,
        created_at: session.created_at,
        updated_at: session.updated_at,
        student_cid: session.student_cid,
        student_name: session.student_name,
        instructor_cid: session.instructor_cid,
        instructor_name: session.instructor_name,
        tickets,
        performance_indicator,
        additional_trainers,
    }))
}

pub async fn fetch_performance_indicator(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<TrainingSessionPerformanceIndicatorDetail>, ApiError> {
    let Some(root) = fetch_performance_indicator_root(pool, session_id).await? else {
        return Ok(None);
    };

    let category_rows = fetch_performance_indicator_categories(pool, &root.id).await?;
    let criteria_rows = fetch_performance_indicator_criteria(pool, &root.id).await?;

    let mut criteria_by_category: std::collections::HashMap<
        String,
        Vec<TrainingSessionPerformanceIndicatorCriteriaDetail>,
    > = std::collections::HashMap::new();
    for row in criteria_rows {
        criteria_by_category
            .entry(row.category_id)
            .or_default()
            .push(TrainingSessionPerformanceIndicatorCriteriaDetail {
                id: row.id,
                name: row.name,
                order: row.sort_order,
                marker: row.marker,
                comments: row.comments,
            });
    }

    let categories = category_rows
        .into_iter()
        .map(|row| TrainingSessionPerformanceIndicatorCategoryDetail {
            id: row.id.clone(),
            name: row.name,
            order: row.sort_order,
            criteria: criteria_by_category.remove(&row.id).unwrap_or_default(),
        })
        .collect();

    Ok(Some(TrainingSessionPerformanceIndicatorDetail {
        id: root.id,
        categories,
    }))
}
