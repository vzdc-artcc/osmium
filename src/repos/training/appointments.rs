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
            (
                select count(*)::bigint
                from training.training_appointment_additional_trainers aat
                where aat.appointment_id = ta.id
            ) as additional_trainer_count,
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

    sqlx::query_as::<_, TrainingAppointmentListItem>(&sql)
        .bind(trainer_id)
        .bind(student_id)
        .bind(user_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
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
