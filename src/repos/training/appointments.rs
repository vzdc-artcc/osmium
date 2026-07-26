use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres, Transaction};

use crate::{
    errors::ApiError,
    models::{
        AdditionalTrainerDetail, TrainingAppointmentDetail, TrainingAppointmentLessonSummary,
        TrainingAppointmentListItem,
    },
};

#[derive(Debug, sqlx::FromRow)]
pub struct AppointmentDetailRow {
    pub id: String,
    pub student_id: String,
    pub trainer_id: String,
    pub start: DateTime<Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub warning_email_sent: bool,
    pub atc_booking_id: Option<String>,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
}

pub async fn count_appointments(
    pool: &PgPool,
    trainer_id: Option<&str>,
    student_id: Option<&str>,
    user_id: Option<&str>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from training.training_appointments ta
        where ($1::text is null or ta.trainer_id = $1)
          and ($2::text is null or ta.student_id = $2)
          and ($3::text is null or (ta.trainer_id = $3 or ta.student_id = $3))
        "#,
    )
    .bind(trainer_id)
    .bind(student_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, sqlx::FromRow)]
struct AppointmentListRow {
    id: String,
    student_id: String,
    trainer_id: String,
    start: DateTime<Utc>,
    environment: Option<String>,
    double_booking: bool,
    preparation_completed: bool,
    warning_email_sent: bool,
    atc_booking_id: Option<String>,
    notes: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    student_cid: i64,
    student_name: String,
    trainer_cid: i64,
    trainer_name: String,
    lesson_count: i64,
    lessons_json: serde_json::Value,
    additional_trainer_count: i64,
    additional_trainers_json: serde_json::Value,
    estimated_duration_minutes: Option<i64>,
    estimated_end: Option<DateTime<Utc>>,
}

impl AppointmentListRow {
    fn into_model(self) -> Result<TrainingAppointmentListItem, ApiError> {
        let lessons = serde_json::from_value(self.lessons_json).map_err(|_| ApiError::Internal)?;
        let additional_trainers =
            serde_json::from_value(self.additional_trainers_json).map_err(|_| ApiError::Internal)?;

        Ok(TrainingAppointmentListItem {
            id: self.id,
            student_id: self.student_id,
            trainer_id: self.trainer_id,
            start: self.start,
            environment: self.environment,
            double_booking: self.double_booking,
            preparation_completed: self.preparation_completed,
            warning_email_sent: self.warning_email_sent,
            atc_booking_id: self.atc_booking_id,
            notes: self.notes,
            created_at: self.created_at,
            updated_at: self.updated_at,
            student_cid: self.student_cid,
            student_name: self.student_name,
            trainer_cid: self.trainer_cid,
            trainer_name: self.trainer_name,
            lesson_count: self.lesson_count,
            lessons,
            additional_trainer_count: self.additional_trainer_count,
            additional_trainers,
            estimated_duration_minutes: self.estimated_duration_minutes,
            estimated_end: self.estimated_end,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn list_appointments(
    pool: &PgPool,
    trainer_id: Option<&str>,
    student_id: Option<&str>,
    user_id: Option<&str>,
    sort_column: &str,
    sort_direction: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<TrainingAppointmentListItem>, ApiError> {
    let sql = format!(
        r#"
        select
            ta.id,
            ta.student_id,
            ta.trainer_id,
            ta.start,
            ta.environment,
            ta.double_booking,
            ta.preparation_completed,
            ta.warning_email_sent,
            ta.atc_booking_id,
            ta.notes,
            ta.created_at,
            ta.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            tu.cid as trainer_cid,
            tu.full_name as trainer_name,
            count(tal.lesson_id)::bigint as lesson_count,
            coalesce(
                (
                    select json_agg(json_build_object(
                        'id', l2.id,
                        'identifier', l2.identifier,
                        'name', l2.name,
                        'location', l2.location,
                        'duration', l2.duration
                    ) order by l2.location asc, l2.identifier asc, l2.name asc, l2.id asc)
                    from training.training_appointment_lessons tal2
                    join training.lessons l2 on l2.id = tal2.lesson_id
                    where tal2.appointment_id = ta.id
                ),
                '[]'::json
            ) as lessons_json,
            (
                select count(*)::bigint
                from training.training_appointment_additional_trainers aat
                where aat.appointment_id = ta.id
            ) as additional_trainer_count,
            coalesce(
                (
                    select json_agg(json_build_object(
                        'trainer_id', aat2.trainer_id,
                        'trainer_cid', atu.cid,
                        'trainer_name', atu.full_name,
                        'description', aat2.description
                    ) order by atu.full_name asc, aat2.trainer_id asc)
                    from training.training_appointment_additional_trainers aat2
                    join identity.users atu on atu.id = aat2.trainer_id
                    where aat2.appointment_id = ta.id
                ),
                '[]'::json
            ) as additional_trainers_json,
            case
                when count(tal.lesson_id) = 0 then null
                else sum(l.duration)::bigint
            end as estimated_duration_minutes,
            case
                when count(tal.lesson_id) = 0 then null
                else ta.start + make_interval(mins => sum(l.duration)::int)
            end as estimated_end
        from training.training_appointments ta
        join identity.users su on su.id = ta.student_id
        join identity.users tu on tu.id = ta.trainer_id
        left join training.training_appointment_lessons tal on tal.appointment_id = ta.id
        left join training.lessons l on l.id = tal.lesson_id
        where ($1::text is null or ta.trainer_id = $1)
          and ($2::text is null or ta.student_id = $2)
          and ($3::text is null or (ta.trainer_id = $3 or ta.student_id = $3))
        group by
            ta.id, su.cid, su.full_name, tu.cid, tu.full_name
        order by {sort_column} {sort_direction}, ta.id asc
        limit $4 offset $5
        "#
    );

    sqlx::query_as::<_, AppointmentListRow>(&sql)
        .bind(trainer_id)
        .bind(student_id)
        .bind(user_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .into_iter()
        .map(AppointmentListRow::into_model)
        .collect()
}

fn estimate_appointment_end(
    start: DateTime<Utc>,
    lessons: &[TrainingAppointmentLessonSummary],
) -> (Option<i64>, Option<DateTime<Utc>>) {
    if lessons.is_empty() {
        return (None, None);
    }

    let total_minutes = lessons
        .iter()
        .map(|lesson| i64::from(lesson.duration))
        .sum();
    (
        Some(total_minutes),
        Some(start + chrono::Duration::minutes(total_minutes)),
    )
}

pub async fn fetch_appointment_detail(
    pool: &PgPool,
    appointment_id: &str,
) -> Result<Option<TrainingAppointmentDetail>, ApiError> {
    let appointment = fetch_appointment_row(pool, appointment_id).await?;

    let Some(appointment) = appointment else {
        return Ok(None);
    };

    let lessons = sqlx::query_as::<_, TrainingAppointmentLessonSummary>(
        r#"
        select
            l.id,
            l.identifier,
            l.name,
            l.location,
            l.duration
        from training.training_appointment_lessons tal
        join training.lessons l on l.id = tal.lesson_id
        where tal.appointment_id = $1
        order by l.location asc, l.identifier asc, l.name asc, l.id asc
        "#,
    )
    .bind(appointment_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let (estimated_duration_minutes, estimated_end) =
        estimate_appointment_end(appointment.start, &lessons);

    let additional_trainers = fetch_appointment_additional_trainers(pool, appointment_id).await?;

    Ok(Some(TrainingAppointmentDetail {
        id: appointment.id,
        student_id: appointment.student_id,
        trainer_id: appointment.trainer_id,
        start: appointment.start,
        environment: appointment.environment,
        double_booking: appointment.double_booking,
        preparation_completed: appointment.preparation_completed,
        warning_email_sent: appointment.warning_email_sent,
        atc_booking_id: appointment.atc_booking_id,
        notes: appointment.notes,
        created_at: appointment.created_at,
        updated_at: appointment.updated_at,
        student_cid: appointment.student_cid,
        student_name: appointment.student_name,
        trainer_cid: appointment.trainer_cid,
        trainer_name: appointment.trainer_name,
        estimated_duration_minutes,
        estimated_end,
        lessons,
        additional_trainers,
    }))
}

pub async fn fetch_appointment_row<'e, E>(
    executor: E,
    appointment_id: &str,
) -> Result<Option<AppointmentDetailRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, AppointmentDetailRow>(
        r#"
        select
            ta.id,
            ta.student_id,
            ta.trainer_id,
            ta.start,
            ta.environment,
            ta.double_booking,
            ta.preparation_completed,
            ta.warning_email_sent,
            ta.atc_booking_id,
            ta.notes,
            ta.created_at,
            ta.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            tu.cid as trainer_cid,
            tu.full_name as trainer_name
        from training.training_appointments ta
        join identity.users su on su.id = ta.student_id
        join identity.users tu on tu.id = ta.trainer_id
        where ta.id = $1
        "#,
    )
    .bind(appointment_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_appointment_additional_trainers<'e, E>(
    executor: E,
    appointment_id: &str,
) -> Result<Vec<AdditionalTrainerDetail>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, AdditionalTrainerDetail>(
        r#"
        select
            aat.trainer_id,
            u.cid as trainer_cid,
            u.full_name as trainer_name,
            aat.description
        from training.training_appointment_additional_trainers aat
        join identity.users u on u.id = aat.trainer_id
        where aat.appointment_id = $1
        order by u.full_name asc, aat.trainer_id asc
        "#,
    )
    .bind(appointment_id)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_appointment_additional_trainers(
    tx: &mut Transaction<'_, Postgres>,
    appointment_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "delete from training.training_appointment_additional_trainers where appointment_id = $1",
    )
    .bind(appointment_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_appointment_additional_trainer_row(
    tx: &mut Transaction<'_, Postgres>,
    appointment_id: &str,
    trainer_id: &str,
    description: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into training.training_appointment_additional_trainers (appointment_id, trainer_id, description)
        values ($1, $2, $3)
        "#,
    )
    .bind(appointment_id)
    .bind(trainer_id)
    .bind(description)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
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

pub async fn fetch_appointment_lesson_ids<'e, E>(
    executor: E,
    appointment_id: &str,
) -> Result<Vec<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>(
        r#"
        select lesson_id
        from training.training_appointment_lessons
        where appointment_id = $1
        order by lesson_id asc
        "#,
    )
    .bind(appointment_id)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn user_exists<'e, E>(executor: E, user_id: &str) -> Result<Option<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>("select id from identity.users where id = $1")
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn resolve_appointment_lessons<'e, E>(
    executor: E,
    lesson_ids: &[String],
) -> Result<Vec<TrainingAppointmentLessonSummary>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let lessons = sqlx::query_as::<_, TrainingAppointmentLessonSummary>(
        r#"
        select id, identifier, name, location, duration
        from training.lessons
        where id = any($1)
        "#,
    )
    .bind(lesson_ids)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    if lessons.len() != lesson_ids.len() {
        return Err(ApiError::BadRequest);
    }

    Ok(lessons)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_appointment<'e, E>(
    executor: E,
    id: &str,
    student_id: &str,
    trainer_id: &str,
    start: DateTime<Utc>,
    environment: Option<&str>,
    notes: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into training.training_appointments (
            id,
            student_id,
            trainer_id,
            start,
            environment,
            notes,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $7)
        "#,
    )
    .bind(id)
    .bind(student_id)
    .bind(trainer_id)
    .bind(start)
    .bind(environment)
    .bind(notes)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn replace_appointment_lessons(
    tx: &mut Transaction<'_, Postgres>,
    appointment_id: &str,
    lesson_ids: &[String],
) -> Result<(), ApiError> {
    sqlx::query("delete from training.training_appointment_lessons where appointment_id = $1")
        .bind(appointment_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    for lesson_id in lesson_ids {
        sqlx::query(
            r#"
            insert into training.training_appointment_lessons (appointment_id, lesson_id)
            values ($1, $2)
            "#,
        )
        .bind(appointment_id)
        .bind(lesson_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_appointment_row<'e, E>(
    executor: E,
    appointment_id: &str,
    student_id: &str,
    start: DateTime<Utc>,
    environment: Option<&str>,
    double_booking: bool,
    preparation_completed: bool,
    warning_email_sent: bool,
    atc_booking_id: Option<&str>,
    notes: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update training.training_appointments
        set
            student_id = $2,
            start = $3,
            environment = $4,
            double_booking = $5,
            preparation_completed = $6,
            warning_email_sent = $7,
            atc_booking_id = $8,
            notes = $9,
            updated_at = $10
        where id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(student_id)
    .bind(start)
    .bind(environment)
    .bind(double_booking)
    .bind(preparation_completed)
    .bind(warning_email_sent)
    .bind(atc_booking_id)
    .bind(notes)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn delete_appointment_row<'e, E>(
    executor: E,
    appointment_id: &str,
) -> Result<Option<AppointmentDetailRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, AppointmentDetailRow>(
        r#"
        delete from training.training_appointments ta
        using identity.users su, identity.users tu
        where ta.id = $1
          and su.id = ta.student_id
          and tu.id = ta.trainer_id
        returning
            ta.id,
            ta.student_id,
            ta.trainer_id,
            ta.start,
            ta.environment,
            ta.double_booking,
            ta.preparation_completed,
            ta.warning_email_sent,
            ta.atc_booking_id,
            ta.notes,
            ta.created_at,
            ta.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            tu.cid as trainer_cid,
            tu.full_name as trainer_name
        "#,
    )
    .bind(appointment_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

/// A single appointment's shape as needed by the environment-assignment
/// sweep — bounded to what the round-robin algorithm actually reads, not
/// the full appointment record. `all_live`/`all_classroom` are computed in
/// SQL (`bool_and` over each lesson's `location`) rather than by shipping
/// the full lesson list to Rust and checking there, matching the same
/// "compute in SQL" style already used for `estimated_duration_minutes` in
/// `list_appointments`. An appointment with no lessons at all has
/// `all_live`/`all_classroom` both `false` (aggregate over zero rows), so
/// it falls through to round-robin assignment like the original site did.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppointmentSyncRow {
    pub id: String,
    pub start: DateTime<Utc>,
    pub duration_minutes: i64,
    pub all_live: bool,
    pub all_classroom: bool,
}

/// Appointments starting from `now` onward, ordered by start ascending —
/// the exact set the environment round-robin needs to walk in order.
/// Unlike the legacy website cron (which fetched from 24h in the past
/// onward and then skipped past appointments inside the loop), this only
/// fetches what the algorithm actually uses: past appointments never
/// affected the round-robin state there either, since the skip happened
/// before any `previousAssignments` bookkeeping.
pub async fn list_future_appointments_for_sync(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> Result<Vec<AppointmentSyncRow>, ApiError> {
    sqlx::query_as::<_, AppointmentSyncRow>(
        r#"
        select
            ta.id,
            ta.start,
            coalesce(sum(l.duration), 0)::bigint as duration_minutes,
            coalesce(bool_and(l.location = 1), false) as all_live,
            coalesce(bool_and(l.location = 0), false) as all_classroom
        from training.training_appointments ta
        left join training.training_appointment_lessons tal on tal.appointment_id = ta.id
        left join training.lessons l on l.id = tal.lesson_id
        where ta.start >= $1
        group by ta.id, ta.start
        order by ta.start asc
        "#,
    )
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Targeted update for the sync sweep — only touches `environment` and
/// `double_booking`, unlike `update_appointment_row`'s full-replace
/// semantics (which would require resending every other field).
pub async fn update_appointment_environment(
    pool: &PgPool,
    appointment_id: &str,
    environment: &str,
    double_booking: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update training.training_appointments
        set environment = $2,
            double_booking = $3,
            updated_at = $4
        where id = $1
        "#,
    )
    .bind(appointment_id)
    .bind(environment)
    .bind(double_booking)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AppointmentWarningRow {
    pub id: String,
    pub start: DateTime<Utc>,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_id: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
}

/// Appointments starting within the next `until` window that haven't had
/// their advance warning email sent yet.
pub async fn list_appointments_needing_warning_email(
    pool: &PgPool,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Vec<AppointmentWarningRow>, ApiError> {
    sqlx::query_as::<_, AppointmentWarningRow>(
        r#"
        select
            ta.id,
            ta.start,
            ta.student_id,
            su.cid as student_cid,
            su.full_name as student_name,
            ta.trainer_id,
            tu.cid as trainer_cid,
            tu.full_name as trainer_name
        from training.training_appointments ta
        join identity.users su on su.id = ta.student_id
        join identity.users tu on tu.id = ta.trainer_id
        where ta.start >= $1
          and ta.start <= $2
          and ta.warning_email_sent = false
        order by ta.start asc
        "#,
    )
    .bind(now)
    .bind(until)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn mark_warning_email_sent(pool: &PgPool, appointment_id: &str) -> Result<(), ApiError> {
    sqlx::query(
        "update training.training_appointments set warning_email_sent = true, updated_at = now() where id = $1",
    )
    .bind(appointment_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}
