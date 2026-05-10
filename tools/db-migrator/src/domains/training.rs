use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, record_warning},
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

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAssignmentOtherTrainer {
    assignment_id: String,
    trainer_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAssignmentRequest {
    id: String,
    student_id: String,
    submitted_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAssignmentRequestInterestedTrainer {
    assignment_request_id: String,
    trainer_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceReleaseRequest {
    id: String,
    student_id: String,
    submitted_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceAppointment {
    id: String,
    student_id: String,
    trainer_id: String,
    start: NaiveDateTime,
    environment: Option<String>,
    double_booking: bool,
    preparation_completed: bool,
    warning_email_sent: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
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
    created_at: NaiveDateTime,
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
    start: NaiveDateTime,
    end: NaiveDateTime,
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
    training_ticket_id: String,
    criteria_id: String,
    cell_id: String,
    passed: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTicketCommonMistake {
    id: String,
    name: String,
    description: String,
    facility: Option<String>,
    training_ticket_id: String,
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
        r#"
        select
            id,
            "studentId" as student_id,
            "primaryTrainerId" as primary_trainer_id
        from public."TrainingAssignment"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let other_trainers = sqlx::query_as::<_, SourceAssignmentOtherTrainer>(
        r#"
        select
            "A" as assignment_id,
            "B" as trainer_id
        from public."_TrainingAssignmentOtherTrainers"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in assignments {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let primary_trainer_id = mapped_id(&state.target, "user", &row.primary_trainer_id).await?;
        let target_id = mapped_or_same(&state.target, "training_assignment", &row.id).await?;
        let existed = exists(&state.target, "training.training_assignments", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_assignments (id, student_id, primary_trainer_id)
                values ($1, $2, $3)
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    primary_trainer_id = excluded.primary_trainer_id
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
                &format!("student:{student_id}"),
                &target_id,
                &format!("student:{student_id}"),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    if !state.config.dry_run {
        sqlx::query("delete from training.training_assignment_other_trainers")
            .execute(&state.target)
            .await?;
        for row in other_trainers {
            let assignment_id =
                mapped_id(&state.target, "training_assignment", &row.assignment_id).await?;
            let trainer_id = mapped_id(&state.target, "user", &row.trainer_id).await?;
            sqlx::query(
                r#"
                insert into training.training_assignment_other_trainers (assignment_id, trainer_id)
                values ($1, $2)
                on conflict do nothing
                "#,
            )
            .bind(&assignment_id)
            .bind(&trainer_id)
            .execute(&state.target)
            .await?;
        }
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
        r#"
        select
            id,
            "studentId" as student_id,
            "submittedAt" as submitted_at
        from public."TrainingAssignmentRequest"
        order by "submittedAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let interested = sqlx::query_as::<_, SourceAssignmentRequestInterestedTrainer>(
        r#"
        select
            "A" as assignment_request_id,
            "B" as trainer_id
        from public."_TrainingAssignmentRequestInterestedTrainers"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in requests {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let target_id =
            mapped_or_same(&state.target, "training_assignment_request", &row.id).await?;
        let existed = exists(
            &state.target,
            "training.training_assignment_requests",
            &target_id,
        )
        .await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_assignment_requests (id, student_id, submitted_at, status)
                values ($1, $2, $3, 'PENDING')
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    submitted_at = excluded.submitted_at,
                    status = excluded.status
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(assume_utc(row.submitted_at))
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_assignment_request",
                &row.id,
                &format!("student:{student_id}"),
                &target_id,
                &format!("student:{student_id}"),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    if !state.config.dry_run {
        sqlx::query("delete from training.training_assignment_request_interested_trainers")
            .execute(&state.target)
            .await?;
        for row in interested {
            let assignment_request_id = mapped_id(
                &state.target,
                "training_assignment_request",
                &row.assignment_request_id,
            )
            .await?;
            let trainer_id = mapped_id(&state.target, "user", &row.trainer_id).await?;
            sqlx::query(
                r#"
                insert into training.training_assignment_request_interested_trainers (
                    assignment_request_id, trainer_id
                )
                values ($1, $2)
                on conflict do nothing
                "#,
            )
            .bind(&assignment_request_id)
            .bind(&trainer_id)
            .execute(&state.target)
            .await?;
        }
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
        r#"
        select
            id,
            "studentId" as student_id,
            "submittedAt" as submitted_at
        from public."TrainerReleaseRequest"
        order by "submittedAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let target_id = mapped_or_same(&state.target, "trainer_release_request", &row.id).await?;
        let existed = exists(
            &state.target,
            "training.trainer_release_requests",
            &target_id,
        )
        .await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.trainer_release_requests (id, student_id, submitted_at, status)
                values ($1, $2, $3, 'PENDING')
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    submitted_at = excluded.submitted_at,
                    status = excluded.status
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(assume_utc(row.submitted_at))
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "trainer_release_request",
                &row.id,
                &format!("student:{student_id}"),
                &target_id,
                &format!("student:{student_id}"),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "trainer_release_requests").await
}

async fn migrate_appointments(state: &mut AppState) -> Result<()> {
    let appointments = sqlx::query_as::<_, SourceAppointment>(
        r#"
        select
            id,
            "studentId" as student_id,
            "trainerId" as trainer_id,
            start,
            environment,
            "doubleBooking" as double_booking,
            "preparationCompleted" as preparation_completed,
            "warningEmailSent" as warning_email_sent
        from public."TrainingAppointment"
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let lessons = sqlx::query_as::<_, SourceAppointmentLesson>(
        r#"
        select
            "A" as appointment_id,
            "B" as lesson_id
        from public."_LessonToTrainingAppointment"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in appointments {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let trainer_id = mapped_id(&state.target, "user", &row.trainer_id).await?;
        let target_id = mapped_or_same(&state.target, "training_appointment", &row.id).await?;
        let existed = exists(&state.target, "training.training_appointments", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_appointments (
                    id, student_id, trainer_id, start, environment, double_booking,
                    preparation_completed, warning_email_sent
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    trainer_id = excluded.trainer_id,
                    start = excluded.start,
                    environment = excluded.environment,
                    double_booking = excluded.double_booking,
                    preparation_completed = excluded.preparation_completed,
                    warning_email_sent = excluded.warning_email_sent
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&trainer_id)
            .bind(assume_utc(row.start))
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
                &format!("{student_id}:{trainer_id}:{}", row.start),
                &target_id,
                &format!("{student_id}:{trainer_id}:{}", row.start),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    if !state.config.dry_run {
        sqlx::query("delete from training.training_appointment_lessons")
            .execute(&state.target)
            .await?;
        for row in lessons {
            let appointment_id =
                mapped_id(&state.target, "training_appointment", &row.appointment_id).await?;
            let lesson_id = mapped_id(&state.target, "lesson", &row.lesson_id).await?;
            if !exists(
                &state.target,
                "training.training_appointments",
                &appointment_id,
            )
            .await?
            {
                record_warning(
                    state,
                    DOMAIN,
                    "training_appointment_lesson",
                    &format!("{}:{}", row.appointment_id, row.lesson_id),
                    format!(
                        "skipping lesson link because appointment `{}` resolved to missing target appointment `{appointment_id}`",
                        row.appointment_id
                    ),
                )
                .await?;
                continue;
            }
            if !exists(&state.target, "training.lessons", &lesson_id).await? {
                record_warning(
                    state,
                    DOMAIN,
                    "training_appointment_lesson",
                    &format!("{}:{}", row.appointment_id, row.lesson_id),
                    format!(
                        "skipping lesson link because lesson `{}` resolved to missing target lesson `{lesson_id}`",
                        row.lesson_id
                    ),
                )
                .await?;
                continue;
            }
            sqlx::query(
                r#"
                insert into training.training_appointment_lessons (appointment_id, lesson_id)
                values ($1, $2)
                on conflict do nothing
                "#,
            )
            .bind(&appointment_id)
            .bind(&lesson_id)
            .execute(&state.target)
            .await?;
        }
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
        r#"
        select
            id,
            "studentId" as student_id,
            "assignedInstructorId" as assigned_instructor_id,
            notes,
            "createdAt" as created_at
        from public."OtsRecommendation"
        order by "createdAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let assigned_instructor_id = if let Some(id) = row.assigned_instructor_id.as_deref() {
            Some(mapped_id(&state.target, "user", id).await?)
        } else {
            None
        };
        let target_id = mapped_or_same(&state.target, "ots_recommendation", &row.id).await?;
        let existed = exists(&state.target, "training.ots_recommendations", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.ots_recommendations (
                    id, student_id, assigned_instructor_id, notes, created_at
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    assigned_instructor_id = excluded.assigned_instructor_id,
                    notes = excluded.notes,
                    created_at = excluded.created_at
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&assigned_instructor_id)
            .bind(&row.notes)
            .bind(assume_utc(row.created_at))
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ots_recommendation",
                &row.id,
                &format!("{student_id}:{}", row.created_at),
                &target_id,
                &format!("{student_id}:{}", row.created_at),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "ots_recommendations").await
}

async fn migrate_user_progressions(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceUserProgression>(
        r#"
        select id as user_id, "trainingProgressionId" as progression_id
        from public."User"
        where "trainingProgressionId" is not null
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let progression_id =
            mapped_id(&state.target, "training_progression", &row.progression_id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.user_progressions (user_id, progression_id)
                values ($1, $2)
                on conflict (user_id) do update set
                    progression_id = excluded.progression_id
                "#,
            )
            .bind(&user_id)
            .bind(&progression_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "user_progression",
                &row.user_id,
                &format!("{user_id}:{progression_id}"),
                &user_id,
                &format!("{user_id}:{progression_id}"),
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "user_progressions").await
}

async fn migrate_training_sessions(state: &mut AppState) -> Result<()> {
    let sessions = sqlx::query_as::<_, SourceTrainingSession>(
        r#"
        select
            id,
            "studentId" as student_id,
            "instructorId" as instructor_id,
            start,
            "end" as end,
            "additionalComments" as additional_comments,
            "trainerComments" as trainer_comments,
            "vatusaId" as vatusa_id,
            "enableMarkdown" as enable_markdown
        from public."TrainingSession"
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let tickets = sqlx::query_as::<_, SourceTrainingTicket>(
        r#"
        select
            id,
            "sessionId" as session_id,
            "lessonId" as lesson_id,
            passed
        from public."TrainingTicket"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let rubric_scores = sqlx::query_as::<_, SourceRubricScore>(
        r#"
        select
            id,
            "trainingTicketId" as training_ticket_id,
            "criteriaId" as criteria_id,
            "cellId" as cell_id,
            passed
        from public."RubricCriteraScore"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let common_mistakes = sqlx::query_as::<_, SourceTicketCommonMistake>(
        r#"
        select
            id,
            name,
            description,
            facility,
            "trainingTicketId" as training_ticket_id
        from public."CommonMistake"
        where "trainingTicketId" is not null
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let performance_indicators = sqlx::query_as::<_, SourceSessionPi>(
        r#"
        select
            id,
            "sessionId" as training_session_id
        from public."TrainingSessionPerformanceIndicator"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let pi_categories = sqlx::query_as::<_, SourceSessionPiCategory>(
        r#"
        select
            id,
            "sessionId" as session_performance_indicator_id,
            name,
            "order" as sort_order
        from public."TrainingSessionPerformanceIndicatorCategory"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let pi_criteria = sqlx::query_as::<_, SourceSessionPiCriteria>(
        r#"
        select
            id,
            "categoryId" as category_id,
            name,
            "order" as sort_order,
            marker::text as marker,
            comments
        from public."TrainingSessionPerformanceIndicatorCriteria"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in sessions {
        state.report.domain_mut(DOMAIN).planned += 1;
        let student_id = mapped_id(&state.target, "user", &row.student_id).await?;
        let instructor_id = mapped_id(&state.target, "user", &row.instructor_id).await?;
        let target_id = mapped_or_same(&state.target, "training_session", &row.id).await?;
        let existed = exists(&state.target, "training.training_sessions", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_sessions (
                    id, student_id, instructor_id, start, "end", additional_comments,
                    trainer_comments, vatusa_id, enable_markdown
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                on conflict (id) do update set
                    student_id = excluded.student_id,
                    instructor_id = excluded.instructor_id,
                    start = excluded.start,
                    "end" = excluded."end",
                    additional_comments = excluded.additional_comments,
                    trainer_comments = excluded.trainer_comments,
                    vatusa_id = excluded.vatusa_id,
                    enable_markdown = excluded.enable_markdown
                "#,
            )
            .bind(&target_id)
            .bind(&student_id)
            .bind(&instructor_id)
            .bind(assume_utc(row.start))
            .bind(assume_utc(row.end))
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
                &format!("{student_id}:{instructor_id}:{}", row.start),
                &target_id,
                &format!("{student_id}:{instructor_id}:{}", row.start),
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    for row in tickets {
        let session_id = mapped_id(&state.target, "training_session", &row.session_id).await?;
        let lesson_id = mapped_id(&state.target, "lesson", &row.lesson_id).await?;
        let target_id = mapped_or_same(&state.target, "training_ticket", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_tickets (id, session_id, lesson_id, passed)
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    session_id = excluded.session_id,
                    lesson_id = excluded.lesson_id,
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
                &format!("{session_id}:{lesson_id}"),
                &target_id,
                &format!("{session_id}:{lesson_id}"),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in rubric_scores {
        let training_ticket_id =
            mapped_id(&state.target, "training_ticket", &row.training_ticket_id).await?;
        let criteria_id =
            mapped_id(&state.target, "lesson_rubric_criteria", &row.criteria_id).await?;
        let cell_id = mapped_id(&state.target, "lesson_rubric_cell", &row.cell_id).await?;
        let target_id = mapped_or_same(&state.target, "rubric_score", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.rubric_scores (
                    id, training_ticket_id, criteria_id, cell_id, passed
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    training_ticket_id = excluded.training_ticket_id,
                    criteria_id = excluded.criteria_id,
                    cell_id = excluded.cell_id,
                    passed = excluded.passed
                "#,
            )
            .bind(&target_id)
            .bind(&training_ticket_id)
            .bind(&criteria_id)
            .bind(&cell_id)
            .bind(row.passed)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "rubric_score",
                &row.id,
                &format!("{training_ticket_id}:{criteria_id}:{cell_id}"),
                &target_id,
                &format!("{training_ticket_id}:{criteria_id}:{cell_id}"),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in common_mistakes {
        let training_ticket_id =
            mapped_id(&state.target, "training_ticket", &row.training_ticket_id).await?;
        let target_id = mapped_or_same(&state.target, "ticket_common_mistake", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.common_mistakes (id, name, description, facility)
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    name = excluded.name,
                    description = excluded.description,
                    facility = excluded.facility
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.facility)
            .execute(&state.target)
            .await?;
            sqlx::query(
                r#"
                insert into training.training_ticket_common_mistakes (training_ticket_id, common_mistake_id)
                values ($1, $2)
                on conflict do nothing
                "#,
            )
            .bind(&training_ticket_id)
            .bind(&target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ticket_common_mistake",
                &row.id,
                &format!("{training_ticket_id}:{}", row.name),
                &target_id,
                &format!("{training_ticket_id}:{}", row.name),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in performance_indicators {
        let training_session_id =
            mapped_id(&state.target, "training_session", &row.training_session_id).await?;
        let target_id =
            mapped_or_same(&state.target, "session_performance_indicator", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.session_performance_indicators (id, training_session_id)
                values ($1, $2)
                on conflict (id) do update set
                    training_session_id = excluded.training_session_id
                "#,
            )
            .bind(&target_id)
            .bind(&training_session_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "session_performance_indicator",
                &row.id,
                &training_session_id,
                &target_id,
                &training_session_id,
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in pi_categories {
        let session_performance_indicator_id = mapped_id(
            &state.target,
            "session_performance_indicator",
            &row.session_performance_indicator_id,
        )
        .await?;
        let target_id = mapped_or_same(
            &state.target,
            "session_performance_indicator_category",
            &row.id,
        )
        .await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.session_performance_indicator_categories (
                    id, session_performance_indicator_id, name, sort_order
                )
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    session_performance_indicator_id = excluded.session_performance_indicator_id,
                    name = excluded.name,
                    sort_order = excluded.sort_order
                "#,
            )
            .bind(&target_id)
            .bind(&session_performance_indicator_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "session_performance_indicator_category",
                &row.id,
                &format!("{session_performance_indicator_id}:{}", row.sort_order),
                &target_id,
                &format!("{session_performance_indicator_id}:{}", row.sort_order),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in pi_criteria {
        let category_id = mapped_id(
            &state.target,
            "session_performance_indicator_category",
            &row.category_id,
        )
        .await?;
        let target_id = mapped_or_same(
            &state.target,
            "session_performance_indicator_criteria",
            &row.id,
        )
        .await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.session_performance_indicator_criteria (
                    id, category_id, name, sort_order, marker, comments
                )
                values ($1, $2, $3, $4, $5, $6)
                on conflict (id) do update set
                    category_id = excluded.category_id,
                    name = excluded.name,
                    sort_order = excluded.sort_order,
                    marker = excluded.marker,
                    comments = excluded.comments
                "#,
            )
            .bind(&target_id)
            .bind(&category_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(&row.marker)
            .bind(&row.comments)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "session_performance_indicator_criteria",
                &row.id,
                &format!("{category_id}:{}", row.sort_order),
                &target_id,
                &format!("{category_id}:{}", row.sort_order),
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "training_sessions").await
}

async fn mapped_or_same(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .map(|row| row.target_id)
        .unwrap_or_else(|| source_id.to_string()))
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .map(|row| row.target_id)
        .unwrap_or_else(|| source_id.to_string()))
}

async fn exists(pool: &sqlx::PgPool, table: &str, id: &str) -> Result<bool> {
    let query = format!("select exists(select 1 from {table} where id = $1)");
    Ok(sqlx::query_scalar::<_, bool>(&query)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

fn bump_counts(state: &mut AppState, existed: bool) {
    let domain = state.report.domain_mut(DOMAIN);
    if existed {
        domain.updated += 1;
    } else {
        domain.created += 1;
    }
}

async fn checkpoint(state: &AppState, entity_type: &str) -> Result<()> {
    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, entity_type).await?;
    }
    Ok(())
}
