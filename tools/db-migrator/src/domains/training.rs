use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{new_id, record_warning},
    state::AppState,
    target,
};

const DOMAIN: &str = "training";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAssignment {
    id: String,
    student_id: String,
    primary_trainer_id: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceAssignmentOtherTrainer {
    assignment_id: String,
    trainer_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAssignmentRequest {
    id: String,
    student_id: String,
    submitted_at: DateTime<Utc>,
    status: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceAssignmentRequestInterestedTrainer {
    assignment_request_id: String,
    trainer_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceReleaseRequest {
    id: String,
    student_id: String,
    submitted_at: DateTime<Utc>,
    status: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAppointment {
    id: String,
    student_id: String,
    trainer_id: String,
    start: DateTime<Utc>,
    environment: Option<String>,
    double_booking: bool,
    preparation_completed: bool,
    warning_email_sent: bool,
}

#[derive(Debug, Clone, FromRow)]
struct SourceAppointmentLesson {
    appointment_id: String,
    lesson_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceOtsRecommendation {
    id: String,
    student_id: String,
    assigned_instructor_id: Option<String>,
    notes: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceUserProgression {
    user_id: String,
    progression_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTrainingSession {
    id: String,
    student_id: String,
    instructor_id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    additional_comments: Option<String>,
    trainer_comments: Option<String>,
    vatusa_id: Option<String>,
    enable_markdown: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTrainingTicket {
    id: String,
    session_id: String,
    lesson_id: String,
    passed: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceRubricScore {
    id: String,
    training_ticket_id: Option<String>,
    criteria_id: String,
    cell_id: String,
    passed: bool,
}

#[derive(Debug, Clone, FromRow)]
struct SourceTicketMistake {
    training_ticket_id: String,
    common_mistake_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceCommonMistake {
    id: String,
    name: String,
    description: String,
    facility: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSessionPi {
    id: String,
    training_session_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSessionPiCategory {
    id: String,
    session_performance_indicator_id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSessionPiCriteria {
    id: String,
    category_id: String,
    name: String,
    sort_order: i32,
    marker: Option<String>,
    comments: Option<String>,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    migrate_training_assignments(state).await?;
    migrate_assignment_requests(state).await?;
    migrate_release_requests(state).await?;
    migrate_appointments(state).await?;
    migrate_ots_recommendations(state).await?;
    migrate_user_progressions(state).await?;
    migrate_training_sessions(state).await?;
    Ok(())
}

async fn migrate_training_assignments(state: &mut AppState) -> Result<()> {
    let assignments = sqlx::query_as::<_, SourceAssignment>(
        r#"select id, student_id, primary_trainer_id from training.training_assignments"#,
    )
    .fetch_all(&state.source)
    .await?;
    let other_trainers = sqlx::query_as::<_, SourceAssignmentOtherTrainer>(
        r#"select assignment_id, trainer_id from training.training_assignment_other_trainers"#,
    )
    .fetch_all(&state.source)
    .await?;
    let mut others_by_assignment: HashMap<String, Vec<String>> = HashMap::new();
    for row in other_trainers {
        others_by_assignment
            .entry(row.assignment_id)
            .or_default()
            .push(row.trainer_id);
    }

    for row in assignments {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let primary_trainer_id = mapped_id(&state.target, "user", &row.primary_trainer_id).await?;
        let business_key = format!("student:{student_id}");
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "training_assignment", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainingAssignment" where "studentId" = $1"#,
            )
            .bind(&student_id)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingAssignment" (id, "studentId", "primaryTrainerId")
                values ($1, $2, $3)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "primaryTrainerId" = excluded."primaryTrainerId"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&primary_trainer_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_assignment",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;

            sqlx::query(r#"delete from "_TrainingAssignmentOtherTrainers" where "A" = $1"#)
                .bind(&target_id)
                .execute(&state.target)
                .await?;
            for source_trainer_id in others_by_assignment.remove(&row.id).unwrap_or_default() {
                let trainer_id = mapped_id(&state.target, "user", &source_trainer_id).await?;
                sqlx::query(
                    r#"insert into "_TrainingAssignmentOtherTrainers" ("A", "B") values ($1, $2) on conflict do nothing"#,
                )
                .bind(&target_id)
                .bind(&trainer_id)
                .execute(&state.target)
                .await?;
            }
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "training_assignments",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_assignment_requests(state: &mut AppState) -> Result<()> {
    let requests = sqlx::query_as::<_, SourceAssignmentRequest>(
        r#"select id, student_id, submitted_at, status from training.training_assignment_requests order by submitted_at asc"#,
    )
    .fetch_all(&state.source)
    .await?;
    let interested = sqlx::query_as::<_, SourceAssignmentRequestInterestedTrainer>(
        r#"select assignment_request_id, trainer_id from training.training_assignment_request_interested_trainers"#,
    )
    .fetch_all(&state.source)
    .await?;
    let mut interested_by_request: HashMap<String, Vec<String>> = HashMap::new();
    for row in interested {
        interested_by_request
            .entry(row.assignment_request_id)
            .or_default()
            .push(row.trainer_id);
    }

    for row in requests {
        if row.status != "PENDING" {
            record_warning(
                state,
                DOMAIN,
                "training_assignment_request",
                &row.id,
                format!("skipping non-pending request with status {}", row.status),
            )
            .await?;
            state.report.domain_mut(DOMAIN).skipped += 1;
            continue;
        }
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let business_key = format!("student:{student_id}");
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "training_assignment_request", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainingAssignmentRequest" where "studentId" = $1"#,
            )
            .bind(&student_id)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingAssignmentRequest" (id, "studentId", "submittedAt")
                values ($1, $2, $3)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "submittedAt" = excluded."submittedAt"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(row.submitted_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_assignment_request",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;

            sqlx::query(
                r#"delete from "_TrainingAssignmentRequestInterestedTrainers" where "A" = $1"#,
            )
            .bind(&target_id)
            .execute(&state.target)
            .await?;
            for source_trainer_id in interested_by_request.remove(&row.id).unwrap_or_default() {
                let trainer_id = mapped_id(&state.target, "user", &source_trainer_id).await?;
                sqlx::query(
                    r#"insert into "_TrainingAssignmentRequestInterestedTrainers" ("A", "B") values ($1, $2) on conflict do nothing"#,
                )
                .bind(&target_id)
                .bind(&trainer_id)
                .execute(&state.target)
                .await?;
            }
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "training_assignment_requests",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_release_requests(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceReleaseRequest>(
        r#"select id, student_id, submitted_at, status from training.trainer_release_requests order by submitted_at asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        if row.status != "PENDING" {
            record_warning(
                state,
                DOMAIN,
                "trainer_release_request",
                &row.id,
                format!(
                    "skipping non-pending release request with status {}",
                    row.status
                ),
            )
            .await?;
            state.report.domain_mut(DOMAIN).skipped += 1;
            continue;
        }
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let business_key = format!("student:{student_id}");
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "trainer_release_request", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainerReleaseRequest" where "studentId" = $1"#,
            )
            .bind(&student_id)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainerReleaseRequest" (id, "studentId", "submittedAt")
                values ($1, $2, $3)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "submittedAt" = excluded."submittedAt"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(row.submitted_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "trainer_release_request",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "trainer_release_requests",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_appointments(state: &mut AppState) -> Result<()> {
    let appointments = sqlx::query_as::<_, SourceAppointment>(
        r#"
        select id, student_id, trainer_id, start, environment, double_booking, preparation_completed, warning_email_sent
        from training.training_appointments
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let appointment_lessons = sqlx::query_as::<_, SourceAppointmentLesson>(
        r#"select appointment_id, lesson_id from training.training_appointment_lessons"#,
    )
    .fetch_all(&state.source)
    .await?;
    let mut lessons_by_appointment: HashMap<String, Vec<String>> = HashMap::new();
    for row in appointment_lessons {
        lessons_by_appointment
            .entry(row.appointment_id)
            .or_default()
            .push(row.lesson_id);
    }

    for row in appointments {
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let trainer_id = mapped_id(&state.target, "user", &row.trainer_id).await?;
        let business_key = format!("{student_id}:{trainer_id}:{}", row.start.to_rfc3339());
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "training_appointment", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainingAppointment" where "studentId" = $1 and "trainerId" = $2 and start = $3"#,
            )
            .bind(&student_id)
            .bind(&trainer_id)
            .bind(row.start)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingAppointment" (
                    id, "studentId", "trainerId", start, environment, "doubleBooking", "preparationCompleted", "warningEmailSent"
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "trainerId" = excluded."trainerId",
                    start = excluded.start,
                    environment = excluded.environment,
                    "doubleBooking" = excluded."doubleBooking",
                    "preparationCompleted" = excluded."preparationCompleted",
                    "warningEmailSent" = excluded."warningEmailSent"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&trainer_id)
            .bind(row.start)
            .bind(&row.environment)
            .bind(row.double_booking)
            .bind(row.preparation_completed)
            .bind(row.warning_email_sent)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_appointment",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;

            sqlx::query(r#"delete from "_LessonToTrainingAppointment" where "B" = $1"#)
                .bind(&target_id)
                .execute(&state.target)
                .await?;
            for lesson_source_id in lessons_by_appointment.remove(&row.id).unwrap_or_default() {
                let lesson_id = mapped_id(&state.target, "lesson", &lesson_source_id).await?;
                sqlx::query(
                    r#"insert into "_LessonToTrainingAppointment" ("A", "B") values ($1, $2) on conflict do nothing"#,
                )
                .bind(&lesson_id)
                .bind(&target_id)
                .execute(&state.target)
                .await?;
            }
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "training_appointments",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_ots_recommendations(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceOtsRecommendation>(
        r#"select id, student_id, assigned_instructor_id, notes, created_at from training.ots_recommendations order by created_at asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let assigned_instructor_id = match &row.assigned_instructor_id {
            Some(source_user_id) => target::find_mapping(&state.target, "user", source_user_id)
                .await?
                .map(|row| row.target_id),
            None => None,
        };
        let business_key = format!(
            "{student_id}:{}:{}",
            assigned_instructor_id.clone().unwrap_or_default(),
            row.notes
        );
        let target_id = target::find_mapping(&state.target, "ots_recommendation", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "OtsRecommendation" (id, "studentId", "assignedInstructorId", notes, "createdAt")
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "assignedInstructorId" = excluded."assignedInstructorId",
                    notes = excluded.notes,
                    "createdAt" = excluded."createdAt"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&assigned_instructor_id)
            .bind(&row.notes)
            .bind(row.created_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ots_recommendation",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "ots_recommendations",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_user_progressions(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceUserProgression>(
        r#"select user_id, progression_id from training.user_progressions"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let progression_id =
            mapped_id(&state.target, "training_progression", &row.progression_id).await?;
        if !state.config.dry_run {
            sqlx::query(r#"update "User" set "trainingProgressionId" = $2 where id = $1"#)
                .bind(&user_id)
                .bind(&progression_id)
                .execute(&state.target)
                .await?;
        }
    }
    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "user_progressions",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_training_sessions(state: &mut AppState) -> Result<()> {
    let sessions = sqlx::query_as::<_, SourceTrainingSession>(
        r#"
        select id, student_id, instructor_id, start, "end", additional_comments, trainer_comments, vatusa_id, enable_markdown
        from training.training_sessions
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let tickets = sqlx::query_as::<_, SourceTrainingTicket>(
        r#"select id, session_id, lesson_id, passed from training.training_tickets"#,
    )
    .fetch_all(&state.source)
    .await?;
    let scores = sqlx::query_as::<_, SourceRubricScore>(
        r#"select id, training_ticket_id, criteria_id, cell_id, passed from training.rubric_scores"#,
    )
    .fetch_all(&state.source)
    .await?;
    let ticket_mistakes = sqlx::query_as::<_, SourceTicketMistake>(
        r#"select training_ticket_id, common_mistake_id from training.training_ticket_common_mistakes"#,
    )
    .fetch_all(&state.source)
    .await?;
    let common_mistakes = sqlx::query_as::<_, SourceCommonMistake>(
        r#"select id, name, description, facility from training.common_mistakes"#,
    )
    .fetch_all(&state.source)
    .await?;
    let session_pis = sqlx::query_as::<_, SourceSessionPi>(
        r#"select id, training_session_id from training.session_performance_indicators"#,
    )
    .fetch_all(&state.source)
    .await?;
    let session_pi_categories = sqlx::query_as::<_, SourceSessionPiCategory>(
        r#"select id, session_performance_indicator_id, name, sort_order from training.session_performance_indicator_categories"#,
    )
    .fetch_all(&state.source)
    .await?;
    let session_pi_criteria = sqlx::query_as::<_, SourceSessionPiCriteria>(
        r#"select id, category_id, name, sort_order, marker, comments from training.session_performance_indicator_criteria"#,
    )
    .fetch_all(&state.source)
    .await?;

    let common_mistake_by_id = common_mistakes
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut ticket_ids_by_source = HashMap::new();
    let mut session_ids_by_source = HashMap::new();
    let mut session_pi_ids_by_source = HashMap::new();
    let mut session_pi_category_ids_by_source = HashMap::new();

    for row in sessions {
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let instructor_id = mapped_id(&state.target, "user", &row.instructor_id).await?;
        let business_key = format!("{student_id}:{instructor_id}:{}", row.start.to_rfc3339());
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "training_session", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainingSession" where "studentId" = $1 and "instructorId" = $2 and start = $3"#,
            )
            .bind(&student_id)
            .bind(&instructor_id)
            .bind(row.start)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingSession" (
                    id, "studentId", "instructorId", start, "end", "additionalComments",
                    "trainerComments", "vatusaId", "enableMarkdown"
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                on conflict (id) do update set
                    "studentId" = excluded."studentId",
                    "instructorId" = excluded."instructorId",
                    start = excluded.start,
                    "end" = excluded."end",
                    "additionalComments" = excluded."additionalComments",
                    "trainerComments" = excluded."trainerComments",
                    "vatusaId" = excluded."vatusaId",
                    "enableMarkdown" = excluded."enableMarkdown"
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&instructor_id)
            .bind(row.start)
            .bind(row.end)
            .bind(&row.additional_comments)
            .bind(&row.trainer_comments)
            .bind(&row.vatusa_id)
            .bind(row.enable_markdown)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_session",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
        session_ids_by_source.insert(row.id, target_id);
    }

    for row in tickets {
        let session_id = session_ids_by_source
            .get(&row.session_id)
            .cloned()
            .with_context(|| format!("missing migrated training session for ticket {}", row.id))?;
        let lesson_id = mapped_id(&state.target, "lesson", &row.lesson_id).await?;
        let business_key = format!("{session_id}:{lesson_id}");
        let target_id = target::find_mapping(&state.target, "training_ticket", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingTicket" (id, "sessionId", "lessonId", passed)
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    "sessionId" = excluded."sessionId",
                    "lessonId" = excluded."lessonId",
                    passed = excluded.passed
                "#,
            )
            .bind(&target_id)
            .bind(&session_id)
            .bind(&lesson_id)
            .bind(row.passed)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_ticket",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
        ticket_ids_by_source.insert(row.id, target_id);
    }

    for row in scores {
        let Some(ticket_source_id) = row.training_ticket_id.as_ref() else {
            record_warning(
                state,
                DOMAIN,
                "rubric_score",
                &row.id,
                "skipping rubric score without training_ticket_id",
            )
            .await?;
            continue;
        };
        let ticket_id = ticket_ids_by_source
            .get(ticket_source_id)
            .cloned()
            .with_context(|| format!("missing ticket mapping for rubric score {}", row.id))?;
        let criteria_id = mapped_id(&state.target, "lesson_criteria", &row.criteria_id).await?;
        let cell_id = mapped_id(&state.target, "lesson_cell", &row.cell_id).await?;
        let business_key = format!("{ticket_id}:{criteria_id}:{cell_id}");
        let target_id = target::find_mapping(&state.target, "rubric_score", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "RubricCriteraScore" (id, "criteriaId", "cellId", "trainingTicketId", passed)
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    "criteriaId" = excluded."criteriaId",
                    "cellId" = excluded."cellId",
                    "trainingTicketId" = excluded."trainingTicketId",
                    passed = excluded.passed
                "#,
            )
            .bind(&target_id)
            .bind(&criteria_id)
            .bind(&cell_id)
            .bind(&ticket_id)
            .bind(row.passed)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "rubric_score",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in ticket_mistakes {
        let ticket_id = ticket_ids_by_source
            .get(&row.training_ticket_id)
            .cloned()
            .with_context(|| {
                format!(
                    "missing training ticket mapping for mistake link {}",
                    row.common_mistake_id
                )
            })?;
        let source_mistake = common_mistake_by_id
            .get(&row.common_mistake_id)
            .with_context(|| format!("missing common mistake {}", row.common_mistake_id))?;
        let business_key = format!("{ticket_id}:{}", source_mistake.id);
        let target_id = target::find_mapping(&state.target, "ticket_mistake", &business_key)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "CommonMistake" (id, name, description, facility, "trainingTicketId")
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    name = excluded.name,
                    description = excluded.description,
                    facility = excluded.facility,
                    "trainingTicketId" = excluded."trainingTicketId"
                "#,
            )
            .bind(&target_id)
            .bind(&source_mistake.name)
            .bind(&source_mistake.description)
            .bind(&source_mistake.facility)
            .bind(&ticket_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ticket_mistake",
                &business_key,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                source_mistake,
            )
            .await?;
        }
    }

    for row in session_pis {
        let session_id = session_ids_by_source
            .get(&row.training_session_id)
            .cloned()
            .with_context(|| {
                format!("missing training session mapping for session PI {}", row.id)
            })?;
        let business_key = format!("session:{session_id}");
        let target_id = target::find_mapping(&state.target, "training_session_pi", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingSessionPerformanceIndicator" (id, "sessionId")
                values ($1, $2)
                on conflict (id) do update set "sessionId" = excluded."sessionId"
                "#,
            )
            .bind(&target_id)
            .bind(&session_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_session_pi",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
        session_pi_ids_by_source.insert(row.id, target_id);
    }

    for row in session_pi_categories {
        let session_pi_id = session_pi_ids_by_source
            .get(&row.session_performance_indicator_id)
            .cloned()
            .with_context(|| format!("missing session PI mapping for category {}", row.id))?;
        let business_key = format!("{session_pi_id}:{}", row.name);
        let target_id =
            target::find_mapping(&state.target, "training_session_pi_category", &row.id)
                .await?
                .map(|row| row.target_id)
                .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingSessionPerformanceIndicatorCategory" (id, name, "order", "sessionId")
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    name = excluded.name,
                    "order" = excluded."order",
                    "sessionId" = excluded."sessionId"
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(&session_pi_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_session_pi_category",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
        session_pi_category_ids_by_source.insert(row.id, target_id);
    }

    for row in session_pi_criteria {
        let category_id = session_pi_category_ids_by_source
            .get(&row.category_id)
            .cloned()
            .with_context(|| {
                format!(
                    "missing session PI category mapping for criteria {}",
                    row.id
                )
            })?;
        let business_key = format!("{category_id}:{}", row.name);
        let target_id =
            target::find_mapping(&state.target, "training_session_pi_criteria", &row.id)
                .await?
                .map(|row| row.target_id)
                .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingSessionPerformanceIndicatorCriteria" (id, name, "order", marker, comments, "categoryId")
                values ($1, $2, $3, $4::"PerformanceIndicatorMarker", $5, $6)
                on conflict (id) do update set
                    name = excluded.name,
                    "order" = excluded."order",
                    marker = excluded.marker,
                    comments = excluded.comments,
                    "categoryId" = excluded."categoryId"
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(&row.marker)
            .bind(&row.comments)
            .bind(&category_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_session_pi_criteria",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "training_sessions",
        )
        .await?;
    }
    Ok(())
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .with_context(|| format!("missing mapping for {entity_type}/{source_id}"))?
        .target_id)
}
