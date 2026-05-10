use std::collections::HashMap;

use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::assume_utc, mapping::normalize_certification_option, state::AppState, target,
};

const DOMAIN: &str = "reference";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceCertificationType {
    id: String,
    name: String,
    sort_order: i32,
    can_solo_cert: bool,
    auto_assign_unrestricted: bool,
    certification_options: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceEventPreset {
    id: String,
    name: String,
    positions: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTemplate {
    id: String,
    name: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTemplateCategory {
    id: String,
    template_id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceTemplateCriteria {
    id: String,
    category_id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonRubric {
    id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLesson {
    id: String,
    identifier: String,
    location: i32,
    name: String,
    description: String,
    position: String,
    facility: String,
    rubric_id: Option<String>,
    updated_at: NaiveDateTime,
    instructor_only: bool,
    notify_instructor_on_pass: bool,
    release_request_on_pass: bool,
    duration: i32,
    trainee_preparation: Option<String>,
    performance_indicator_template_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonPerformanceIndicator {
    id: String,
    lesson_id: String,
    template_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonCriteria {
    id: String,
    rubric_id: String,
    criteria: String,
    description: String,
    passing: i32,
    max_points: i32,
    sort_order: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonCell {
    id: String,
    criteria_id: String,
    points: i32,
    description: String,
    sort_order: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceCommonMistake {
    id: String,
    name: String,
    description: String,
    facility: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceProgression {
    id: String,
    name: String,
    next_progression_id: Option<String>,
    auto_assign_new_home_obs: bool,
    auto_assign_new_visitor: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceProgressionStep {
    id: String,
    progression_id: String,
    lesson_id: String,
    sort_order: i32,
    optional: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonRosterChange {
    id: String,
    lesson_id: String,
    certification_type_id: String,
    certification_option: String,
    dossier_text: String,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    migrate_certification_types(state).await?;
    migrate_event_presets(state).await?;
    migrate_performance_indicator_templates(state).await?;
    migrate_lessons(state).await?;
    migrate_common_mistakes(state).await?;
    migrate_progressions(state).await?;
    migrate_lesson_roster_changes(state).await?;
    Ok(())
}

async fn migrate_certification_types(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceCertificationType>(
        r#"
        select
            id,
            name,
            "order" as sort_order,
            "canSoloCert" as can_solo_cert,
            "autoAssignUnrestricted" as auto_assign_unrestricted,
            "certificationOptions"::text[] as certification_options
        from public."CertificationType"
        order by "order" asc, name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id = mapped_or_same(&state.target, "certification_type", &row.id).await?;
        let existed = exists(&state.target, "org.certification_types", &target_id).await?;
        let source_business_key = format!("name:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.certification_types (
                    id, name, sort_order, can_solo_cert, auto_assign_unrestricted
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    name = excluded.name,
                    sort_order = excluded.sort_order,
                    can_solo_cert = excluded.can_solo_cert,
                    auto_assign_unrestricted = excluded.auto_assign_unrestricted
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(row.can_solo_cert)
            .bind(row.auto_assign_unrestricted)
            .execute(&state.target)
            .await?;

            sqlx::query(
                r#"delete from org.certification_type_allowed_options where certification_type_id = $1"#,
            )
            .bind(&target_id)
            .execute(&state.target)
            .await?;
            for option in &row.certification_options {
                sqlx::query(
                    r#"
                    insert into org.certification_type_allowed_options (certification_type_id, option_key)
                    values ($1, $2)
                    on conflict do nothing
                    "#,
                )
                .bind(&target_id)
                .bind(normalize_certification_option(option)?)
                .execute(&state.target)
                .await?;
            }

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "certification_type",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "certification_types").await
}

async fn migrate_event_presets(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceEventPreset>(
        r#"
        select id, name, positions
        from public."EventPositionPreset"
        order by name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id = mapped_or_same(&state.target, "event_preset", &row.id).await?;
        let existed = exists(&state.target, "events.event_position_presets", &target_id).await?;
        let source_business_key = format!("name:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into events.event_position_presets (id, name, positions)
                values ($1, $2, $3)
                on conflict (id) do update set
                    name = excluded.name,
                    positions = excluded.positions
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(&row.positions)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event_preset",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "event_position_presets").await
}

async fn migrate_performance_indicator_templates(state: &mut AppState) -> Result<()> {
    let templates = sqlx::query_as::<_, SourceTemplate>(
        r#"select id, name from public."PerformanceIndicatorTemplate" order by name asc"#,
    )
    .fetch_all(&state.source)
    .await?;
    let categories = sqlx::query_as::<_, SourceTemplateCategory>(
        r#"
        select
            id,
            "templateId" as template_id,
            name,
            "order" as sort_order
        from public."PerformanceIndicatorCriteriaCategory"
        order by "templateId" asc, "order" asc, name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let criteria = sqlx::query_as::<_, SourceTemplateCriteria>(
        r#"
        select
            id,
            "categoryId" as category_id,
            name,
            "order" as sort_order
        from public."PerformanceIndicatorCriteria"
        order by "categoryId" asc, "order" asc, name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in templates {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id =
            mapped_or_same(&state.target, "performance_indicator_template", &row.id).await?;
        let existed = exists(
            &state.target,
            "training.performance_indicator_templates",
            &target_id,
        )
        .await?;
        let source_business_key = format!("name:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.performance_indicator_templates (id, name)
                values ($1, $2)
                on conflict (id) do update set
                    name = excluded.name
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "performance_indicator_template",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    for row in categories {
        let template_id = mapped_id(
            &state.target,
            "performance_indicator_template",
            &row.template_id,
        )
        .await?;
        let target_id =
            mapped_or_same(&state.target, "performance_indicator_category", &row.id).await?;
        let existed = exists(
            &state.target,
            "training.performance_indicator_template_categories",
            &target_id,
        )
        .await?;
        let source_business_key = format!("{template_id}:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.performance_indicator_template_categories (
                    id, template_id, name, sort_order
                )
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    template_id = excluded.template_id,
                    name = excluded.name,
                    sort_order = excluded.sort_order
                "#,
            )
            .bind(&target_id)
            .bind(&template_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "performance_indicator_category",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
    }

    for row in criteria {
        let category_id = mapped_id(
            &state.target,
            "performance_indicator_category",
            &row.category_id,
        )
        .await?;
        let target_id =
            mapped_or_same(&state.target, "performance_indicator_criteria", &row.id).await?;
        let existed = exists(
            &state.target,
            "training.performance_indicator_template_criteria",
            &target_id,
        )
        .await?;
        let source_business_key = format!("{category_id}:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.performance_indicator_template_criteria (
                    id, category_id, name, sort_order
                )
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    category_id = excluded.category_id,
                    name = excluded.name,
                    sort_order = excluded.sort_order
                "#,
            )
            .bind(&target_id)
            .bind(&category_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "performance_indicator_criteria",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "performance_indicator_templates").await
}

async fn migrate_lessons(state: &mut AppState) -> Result<()> {
    let rubrics =
        sqlx::query_as::<_, SourceLessonRubric>(r#"select id from public."LessonRubric""#)
            .fetch_all(&state.source)
            .await?;
    let lessons = sqlx::query_as::<_, SourceLesson>(
        r#"
        select
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            "rubricId" as rubric_id,
            "updatedAt" as updated_at,
            "instructorOnly" as instructor_only,
            "notifyInstructorOnPass" as notify_instructor_on_pass,
            "releaseRequestOnPass" as release_request_on_pass,
            duration,
            "traineePreparation" as trainee_preparation,
            "performanceIndicatorId" as performance_indicator_template_id
        from public."Lesson"
        order by identifier asc, location asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let lesson_performance_indicators = sqlx::query_as::<_, SourceLessonPerformanceIndicator>(
        r#"
        select
            id,
            "lessonId" as lesson_id,
            "templateId" as template_id
        from public."LessonPerformanceIndicator"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let rubric_criteria = sqlx::query_as::<_, SourceLessonCriteria>(
        r#"
        select
            id,
            "rubricId" as rubric_id,
            criteria,
            description,
            passing,
            "maxPoints" as max_points,
            row_number() over (partition by "rubricId" order by id) as sort_order
        from public."LessonRubricCriteria"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let rubric_cells = sqlx::query_as::<_, SourceLessonCell>(
        r#"
        select
            id,
            "criteriaId" as criteria_id,
            points,
            description,
            row_number() over (partition by "criteriaId" order by points asc, id asc) as sort_order
        from public."LessonRubricCell"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let template_by_lesson = lesson_performance_indicators
        .into_iter()
        .map(|row| (row.lesson_id, row.template_id))
        .collect::<HashMap<_, _>>();

    for row in rubrics {
        let target_id = mapped_or_same(&state.target, "lesson_rubric", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"insert into training.lesson_rubrics (id) values ($1) on conflict (id) do nothing"#,
            )
            .bind(&target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_rubric",
                &row.id,
                &row.id,
                &target_id,
                &row.id,
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in lessons {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id = mapped_or_same(&state.target, "lesson", &row.id).await?;
        let existed = exists(&state.target, "training.lessons", &target_id).await?;
        let rubric_id = if let Some(id) = row.rubric_id.as_deref() {
            Some(mapped_id(&state.target, "lesson_rubric", id).await?)
        } else {
            None
        };
        let performance_indicator_template_id = if let Some(id) = row
            .performance_indicator_template_id
            .clone()
            .or_else(|| template_by_lesson.get(&row.id).cloned())
        {
            Some(mapped_id(&state.target, "performance_indicator_template", &id).await?)
        } else {
            None
        };
        let source_business_key = format!("{}:{}", row.identifier, row.location);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.lessons (
                    id, identifier, location, name, description, position, facility, rubric_id,
                    updated_at, instructor_only, notify_instructor_on_pass, release_request_on_pass,
                    duration, trainee_preparation, performance_indicator_template_id
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                on conflict (id) do update set
                    identifier = excluded.identifier,
                    location = excluded.location,
                    name = excluded.name,
                    description = excluded.description,
                    position = excluded.position,
                    facility = excluded.facility,
                    rubric_id = excluded.rubric_id,
                    updated_at = excluded.updated_at,
                    instructor_only = excluded.instructor_only,
                    notify_instructor_on_pass = excluded.notify_instructor_on_pass,
                    release_request_on_pass = excluded.release_request_on_pass,
                    duration = excluded.duration,
                    trainee_preparation = excluded.trainee_preparation,
                    performance_indicator_template_id = excluded.performance_indicator_template_id
                "#,
            )
            .bind(&target_id)
            .bind(&row.identifier)
            .bind(row.location)
            .bind(&row.name)
            .bind(&row.description)
            .bind(&row.position)
            .bind(&row.facility)
            .bind(&rubric_id)
            .bind(assume_utc(row.updated_at))
            .bind(row.instructor_only)
            .bind(row.notify_instructor_on_pass)
            .bind(row.release_request_on_pass)
            .bind(row.duration)
            .bind(&row.trainee_preparation)
            .bind(&performance_indicator_template_id)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    for row in rubric_criteria {
        let rubric_id = mapped_id(&state.target, "lesson_rubric", &row.rubric_id).await?;
        let target_id = mapped_or_same(&state.target, "lesson_rubric_criteria", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.lesson_rubric_criteria (
                    id, rubric_id, criteria, description, passing, max_points, sort_order
                )
                values ($1, $2, $3, $4, $5, $6, $7)
                on conflict (id) do update set
                    rubric_id = excluded.rubric_id,
                    criteria = excluded.criteria,
                    description = excluded.description,
                    passing = excluded.passing,
                    max_points = excluded.max_points,
                    sort_order = excluded.sort_order
                "#,
            )
            .bind(&target_id)
            .bind(&rubric_id)
            .bind(&row.criteria)
            .bind(&row.description)
            .bind(row.passing)
            .bind(row.max_points)
            .bind(row.sort_order as i32)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_rubric_criteria",
                &row.id,
                &format!("{rubric_id}:{}", row.criteria),
                &target_id,
                &format!("{rubric_id}:{}", row.criteria),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in rubric_cells {
        let criteria_id =
            mapped_id(&state.target, "lesson_rubric_criteria", &row.criteria_id).await?;
        let target_id = mapped_or_same(&state.target, "lesson_rubric_cell", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.lesson_rubric_cells (
                    id, criteria_id, points, description, sort_order
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    criteria_id = excluded.criteria_id,
                    points = excluded.points,
                    description = excluded.description,
                    sort_order = excluded.sort_order
                "#,
            )
            .bind(&target_id)
            .bind(&criteria_id)
            .bind(row.points)
            .bind(&row.description)
            .bind(row.sort_order as i32)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_rubric_cell",
                &row.id,
                &format!("{criteria_id}:{}", row.points),
                &target_id,
                &format!("{criteria_id}:{}", row.points),
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "lessons").await
}

async fn migrate_common_mistakes(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceCommonMistake>(
        r#"
        select id, name, description, facility
        from public."CommonMistake"
        where "trainingTicketId" is null
        order by name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let target_id = mapped_or_same(&state.target, "common_mistake", &row.id).await?;
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
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "common_mistake",
                &row.id,
                &row.name,
                &target_id,
                &row.name,
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "common_mistakes").await
}

async fn migrate_progressions(state: &mut AppState) -> Result<()> {
    let progressions = sqlx::query_as::<_, SourceProgression>(
        r#"
        select
            id,
            name,
            "nextProgressionId" as next_progression_id,
            "autoAssignNewHomeObs" as auto_assign_new_home_obs,
            "autoAssignNewVisitor" as auto_assign_new_visitor
        from public."TrainingProgression"
        order by name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let steps = sqlx::query_as::<_, SourceProgressionStep>(
        r#"
        select
            id,
            "progressionId" as progression_id,
            "lessonId" as lesson_id,
            "order" as sort_order,
            optional
        from public."TrainingProgressionStep"
        order by "progressionId" asc, "order" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in &progressions {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id = mapped_or_same(&state.target, "training_progression", &row.id).await?;
        let existed = exists(&state.target, "training.training_progressions", &target_id).await?;
        let source_business_key = format!("name:{}", row.name);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_progressions (
                    id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    name = excluded.name,
                    next_progression_id = excluded.next_progression_id,
                    auto_assign_new_home_obs = excluded.auto_assign_new_home_obs,
                    auto_assign_new_visitor = excluded.auto_assign_new_visitor
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(Option::<String>::None)
            .bind(row.auto_assign_new_home_obs)
            .bind(row.auto_assign_new_visitor)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_progression",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    if !state.config.dry_run {
        for row in &progressions {
            let target_id = mapped_or_same(&state.target, "training_progression", &row.id).await?;
            let next_progression_id = if let Some(id) = row.next_progression_id.as_deref() {
                Some(mapped_or_same(&state.target, "training_progression", id).await?)
            } else {
                None
            };

            sqlx::query(
                r#"
                update training.training_progressions
                set next_progression_id = $2
                where id = $1
                "#,
            )
            .bind(&target_id)
            .bind(&next_progression_id)
            .execute(&state.target)
            .await?;
        }
    }

    for row in steps {
        let progression_id =
            mapped_id(&state.target, "training_progression", &row.progression_id).await?;
        let lesson_id = mapped_id(&state.target, "lesson", &row.lesson_id).await?;
        let target_id = if !state.config.dry_run {
            sqlx::query_scalar::<_, String>(
                r#"
                select id
                from training.training_progression_steps
                where progression_id = $1 and sort_order = $2
                "#,
            )
            .bind(&progression_id)
            .bind(row.sort_order)
            .fetch_optional(&state.target)
            .await?
            .unwrap_or_else(|| row.id.clone())
        } else {
            mapped_or_same(&state.target, "training_progression_step", &row.id).await?
        };
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.training_progression_steps (
                    id, progression_id, lesson_id, sort_order, optional
                )
                values ($1, $2, $3, $4, $5)
                on conflict (progression_id, sort_order) do update set
                    lesson_id = excluded.lesson_id,
                    optional = excluded.optional
                "#,
            )
            .bind(&target_id)
            .bind(&progression_id)
            .bind(&lesson_id)
            .bind(row.sort_order)
            .bind(row.optional)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_progression_step",
                &row.id,
                &format!("{progression_id}:{}", row.sort_order),
                &target_id,
                &format!("{progression_id}:{}", row.sort_order),
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "training_progressions").await
}

async fn migrate_lesson_roster_changes(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceLessonRosterChange>(
        r#"
        select
            id,
            "lessonId" as lesson_id,
            "certificationTypeId" as certification_type_id,
            "certificationOption"::text as certification_option,
            "dossierText" as dossier_text
        from public."LessonRosterChange"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let lesson_id = mapped_id(&state.target, "lesson", &row.lesson_id).await?;
        let certification_type_id = mapped_id(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?;
        let target_id = mapped_or_same(&state.target, "lesson_roster_change", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into training.lesson_roster_changes (
                    id, lesson_id, certification_type_id, certification_option, dossier_text
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    lesson_id = excluded.lesson_id,
                    certification_type_id = excluded.certification_type_id,
                    certification_option = excluded.certification_option,
                    dossier_text = excluded.dossier_text
                "#,
            )
            .bind(&target_id)
            .bind(&lesson_id)
            .bind(&certification_type_id)
            .bind(normalize_certification_option(&row.certification_option)?)
            .bind(&row.dossier_text)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_roster_change",
                &row.id,
                &format!("{lesson_id}:{certification_type_id}"),
                &target_id,
                &format!("{lesson_id}:{certification_type_id}"),
                "updated",
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "lesson_roster_changes").await
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
