use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{helpers::new_id, mapping::normalize_certification_option, state::AppState, target};

const DOMAIN: &str = "reference";

#[derive(Debug, Clone, FromRow)]
struct SourceCertificationType {
    id: String,
    name: String,
    sort_order: i32,
    can_solo_cert: bool,
    auto_assign_unrestricted: bool,
}

#[derive(Debug, Clone, FromRow)]
struct SourceCertificationOptionRow {
    certification_type_id: String,
    option_key: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceEventPreset {
    id: String,
    name: String,
    positions: Vec<String>,
}

#[derive(Debug, Clone, FromRow)]
struct SourceTemplate {
    id: String,
    name: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceTemplateCategory {
    id: String,
    template_id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, Clone, FromRow)]
struct SourceTemplateCriteria {
    id: String,
    category_id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, Clone, FromRow)]
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
    updated_at: DateTime<Utc>,
    instructor_only: bool,
    notify_instructor_on_pass: bool,
    release_request_on_pass: bool,
    duration: i32,
    trainee_preparation: Option<String>,
    performance_indicator_template_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonCriteria {
    id: String,
    rubric_id: String,
    criteria: String,
    description: String,
    passing: i32,
    max_points: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLessonCell {
    id: String,
    criteria_id: String,
    points: i32,
    description: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceCommonMistake {
    id: String,
    name: String,
    description: String,
    facility: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
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

#[derive(Debug, Clone, Serialize)]
struct CertificationTypePayload {
    name: String,
    order: i32,
    can_solo_cert: bool,
    auto_assign_unrestricted: bool,
    certification_options: Vec<String>,
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
        select id, name, sort_order, can_solo_cert, auto_assign_unrestricted
        from org.certification_types
        order by sort_order asc, name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let option_rows = sqlx::query_as::<_, SourceCertificationOptionRow>(
        r#"select certification_type_id, option_key from org.certification_type_allowed_options"#,
    )
    .fetch_all(&state.source)
    .await?;

    let mut options_by_type = std::collections::HashMap::<String, Vec<String>>::new();
    for row in option_rows {
        options_by_type
            .entry(row.certification_type_id)
            .or_default()
            .push(normalize_certification_option(&row.option_key)?.to_string());
    }

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let options = options_by_type.remove(&row.id).unwrap_or_default();
        let payload = CertificationTypePayload {
            name: row.name.clone(),
            order: row.sort_order,
            can_solo_cert: row.can_solo_cert,
            auto_assign_unrestricted: row.auto_assign_unrestricted,
            certification_options: options,
        };

        let source_business_key = format!("name:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "certification_type", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(r#"select id from "CertificationType" where name = $1"#)
                .bind(&row.name)
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
                insert into "CertificationType" (
                    id, name, "order", "canSoloCert", "autoAssignUnrestricted", "certificationOptions"
                )
                values ($1, $2, $3, $4, $5, $6::"CertificationOption"[])
                on conflict (id) do update set
                    name = excluded.name,
                    "order" = excluded."order",
                    "canSoloCert" = excluded."canSoloCert",
                    "autoAssignUnrestricted" = excluded."autoAssignUnrestricted",
                    "certificationOptions" = excluded."certificationOptions"
                "#,
            )
            .bind(&target_id)
            .bind(&payload.name)
            .bind(payload.order)
            .bind(payload.can_solo_cert)
            .bind(payload.auto_assign_unrestricted)
            .bind(&payload.certification_options)
            .execute(&state.target)
            .await?;

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
                &payload,
            )
            .await?;
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
            "certification_types",
        )
        .await?;
    }

    Ok(())
}

async fn migrate_event_presets(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceEventPreset>(
        r#"select id, name, positions from events.event_position_presets order by name asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let source_business_key = format!("name:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "event_position_preset", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "EventPositionPreset" where name = $1"#,
            )
            .bind(&row.name)
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
                insert into "EventPositionPreset" (id, name, positions)
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
                "event_position_preset",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row.positions,
            )
            .await?;
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
            "event_position_presets",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_performance_indicator_templates(state: &mut AppState) -> Result<()> {
    let templates = sqlx::query_as::<_, SourceTemplate>(
        r#"select id, name from training.performance_indicator_templates order by name asc"#,
    )
    .fetch_all(&state.source)
    .await?;
    let categories = sqlx::query_as::<_, SourceTemplateCategory>(
        r#"select id, template_id, name, sort_order from training.performance_indicator_template_categories order by sort_order asc, name asc"#,
    )
    .fetch_all(&state.source)
    .await?;
    let criteria = sqlx::query_as::<_, SourceTemplateCriteria>(
        r#"select id, category_id, name, sort_order from training.performance_indicator_template_criteria order by sort_order asc, name asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in templates {
        state.report.domain_mut(DOMAIN).planned += 1;
        let business_key = format!("name:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "pi_template", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "PerformanceIndicatorTemplate" where name = $1"#,
            )
            .bind(&row.name)
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
                r#"insert into "PerformanceIndicatorTemplate" (id, name) values ($1, $2)
                   on conflict (id) do update set name = excluded.name"#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "pi_template",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row.name,
            )
            .await?;
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    for row in categories {
        let template_target_id =
            target::find_mapping(&state.target, "pi_template", &row.template_id)
                .await?
                .context("missing template mapping for PI category")?
                .target_id;
        let business_key = format!("{template_target_id}:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "pi_template_category", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "PerformanceIndicatorCriteriaCategory" where "templateId" = $1 and name = $2"#,
            )
            .bind(&template_target_id)
            .bind(&row.name)
            .fetch_optional(&state.target)
            .await?
        };
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let existed = target::find_mapping(&state.target, "pi_template_category", &row.id)
            .await?
            .is_some();
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "PerformanceIndicatorCriteriaCategory" (id, name, "order", "templateId")
                values ($1, $2, $3, $4)
                on conflict (id) do update set name = excluded.name, "order" = excluded."order", "templateId" = excluded."templateId"
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(&template_target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "pi_template_category",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row.name,
            )
            .await?;
        }
    }

    for row in criteria {
        let category_target_id =
            target::find_mapping(&state.target, "pi_template_category", &row.category_id)
                .await?
                .context("missing category mapping for PI criteria")?
                .target_id;
        let business_key = format!("{category_target_id}:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "pi_template_criteria", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "PerformanceIndicatorCriteria" where "categoryId" = $1 and name = $2"#,
            )
            .bind(&category_target_id)
            .bind(&row.name)
            .fetch_optional(&state.target)
            .await?
        };
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let existed = target::find_mapping(&state.target, "pi_template_criteria", &row.id)
            .await?
            .is_some();
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "PerformanceIndicatorCriteria" (id, name, "order", "categoryId")
                values ($1, $2, $3, $4)
                on conflict (id) do update set name = excluded.name, "order" = excluded."order", "categoryId" = excluded."categoryId"
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
            .bind(row.sort_order)
            .bind(&category_target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "pi_template_criteria",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row.name,
            )
            .await?;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "pi_templates").await?;
    }

    Ok(())
}

async fn migrate_lessons(state: &mut AppState) -> Result<()> {
    let rubrics =
        sqlx::query_as::<_, SourceLessonRubric>(r#"select id from training.lesson_rubrics"#)
            .fetch_all(&state.source)
            .await?;
    let lessons = sqlx::query_as::<_, SourceLesson>(
        r#"
        select id, identifier, location, name, description, position, facility, rubric_id, updated_at,
               instructor_only, notify_instructor_on_pass, release_request_on_pass, duration,
               trainee_preparation, performance_indicator_template_id
        from training.lessons
        order by identifier asc, location asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let criteria = sqlx::query_as::<_, SourceLessonCriteria>(
        r#"select id, rubric_id, criteria, description, passing, max_points from training.lesson_rubric_criteria"#,
    )
    .fetch_all(&state.source)
    .await?;
    let cells = sqlx::query_as::<_, SourceLessonCell>(
        r#"select id, criteria_id, points, description from training.lesson_rubric_cells"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rubrics {
        let target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "lesson_rubric", &row.id).await?
        {
            mapping.target_id
        } else {
            let new = new_id();
            if !state.config.dry_run {
                sqlx::query(
                    r#"insert into "LessonRubric" (id) values ($1) on conflict (id) do nothing"#,
                )
                .bind(&new)
                .execute(&state.target)
                .await?;
                target::upsert_mapping(
                    &state.target,
                    &state.config.run_id,
                    DOMAIN,
                    "lesson_rubric",
                    &row.id,
                    &format!("rubric:{}", row.id),
                    &new,
                    &format!("rubric:{new}"),
                    "created",
                    &row.id,
                )
                .await?;
            }
            new
        };
        let _ = target_id;
    }

    for lesson in lessons {
        state.report.domain_mut(DOMAIN).planned += 1;
        let business_key = format!("{}:{}", lesson.identifier, lesson.location);
        let rubric_target_id = match &lesson.rubric_id {
            Some(source_rubric_id) => Some(
                target::find_mapping(&state.target, "lesson_rubric", source_rubric_id)
                    .await?
                    .context("missing rubric mapping for lesson")?
                    .target_id,
            ),
            None => None,
        };
        let pi_target_id = match &lesson.performance_indicator_template_id {
            Some(template_id) => Some(
                target::find_mapping(&state.target, "pi_template", template_id)
                    .await?
                    .context("missing PI template mapping for lesson")?
                    .target_id,
            ),
            None => None,
        };

        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "lesson", &lesson.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "Lesson" where identifier = $1 and location = $2"#,
            )
            .bind(&lesson.identifier)
            .bind(lesson.location)
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
                insert into "Lesson" (
                    id, identifier, location, name, description, position, facility, "rubricId",
                    "updatedAt", "instructorOnly", "notifyInstructorOnPass", "releaseRequestOnPass",
                    duration, "traineePreparation"
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                on conflict (id) do update set
                    identifier = excluded.identifier,
                    location = excluded.location,
                    name = excluded.name,
                    description = excluded.description,
                    position = excluded.position,
                    facility = excluded.facility,
                    "rubricId" = excluded."rubricId",
                    "updatedAt" = excluded."updatedAt",
                    "instructorOnly" = excluded."instructorOnly",
                    "notifyInstructorOnPass" = excluded."notifyInstructorOnPass",
                    "releaseRequestOnPass" = excluded."releaseRequestOnPass",
                    duration = excluded.duration,
                    "traineePreparation" = excluded."traineePreparation"
                "#,
            )
            .bind(&target_id)
            .bind(&lesson.identifier)
            .bind(lesson.location)
            .bind(&lesson.name)
            .bind(&lesson.description)
            .bind(&lesson.position)
            .bind(&lesson.facility)
            .bind(&rubric_target_id)
            .bind(lesson.updated_at)
            .bind(lesson.instructor_only)
            .bind(lesson.notify_instructor_on_pass)
            .bind(lesson.release_request_on_pass)
            .bind(lesson.duration)
            .bind(&lesson.trainee_preparation)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson",
                &lesson.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &lesson,
            )
            .await?;

            if let Some(template_target_id) = pi_target_id {
                let lesson_pi_id = target::find_mapping(&state.target, "lesson_pi", &lesson.id)
                    .await?
                    .map(|row| row.target_id)
                    .unwrap_or_else(new_id);
                sqlx::query(
                    r#"
                    insert into "LessonPerformanceIndicator" (id, "lessonId", "templateId")
                    values ($1, $2, $3)
                    on conflict ("lessonId") do update set "templateId" = excluded."templateId"
                    "#,
                )
                .bind(&lesson_pi_id)
                .bind(&target_id)
                .bind(&template_target_id)
                .execute(&state.target)
                .await?;
                target::upsert_mapping(
                    &state.target,
                    &state.config.run_id,
                    DOMAIN,
                    "lesson_pi",
                    &lesson.id,
                    &business_key,
                    &lesson_pi_id,
                    &business_key,
                    "updated",
                    &template_target_id,
                )
                .await?;
            }
        }

        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    for row in criteria {
        let rubric_target_id = target::find_mapping(&state.target, "lesson_rubric", &row.rubric_id)
            .await?
            .context("missing rubric mapping for lesson criteria")?
            .target_id;
        let business_key = format!("{rubric_target_id}:{}", row.criteria);
        let target_id = target::find_mapping(&state.target, "lesson_criteria", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "LessonRubricCriteria" (id, "rubricId", criteria, description, passing, "maxPoints")
                values ($1, $2, $3, $4, $5, $6)
                on conflict (id) do update set
                    "rubricId" = excluded."rubricId",
                    criteria = excluded.criteria,
                    description = excluded.description,
                    passing = excluded.passing,
                    "maxPoints" = excluded."maxPoints"
                "#,
            )
            .bind(&target_id)
            .bind(&rubric_target_id)
            .bind(&row.criteria)
            .bind(&row.description)
            .bind(row.passing)
            .bind(row.max_points)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_criteria",
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

    for row in cells {
        let criteria_target_id =
            target::find_mapping(&state.target, "lesson_criteria", &row.criteria_id)
                .await?
                .context("missing criteria mapping for lesson rubric cell")?
                .target_id;
        let business_key = format!("{criteria_target_id}:{}:{}", row.points, row.description);
        let target_id = target::find_mapping(&state.target, "lesson_cell", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "LessonRubricCell" (id, "criteriaId", points, description)
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    "criteriaId" = excluded."criteriaId",
                    points = excluded.points,
                    description = excluded.description
                "#,
            )
            .bind(&target_id)
            .bind(&criteria_target_id)
            .bind(row.points)
            .bind(&row.description)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_cell",
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
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "lessons").await?;
    }

    Ok(())
}

async fn migrate_common_mistakes(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceCommonMistake>(
        r#"select id, name, description, facility from training.common_mistakes order by name asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let business_key = format!("{}:{}", row.name, row.facility.clone().unwrap_or_default());
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "common_mistake", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "CommonMistake" where name = $1 and coalesce(facility, '') = coalesce($2, '') and "trainingTicketId" is null"#,
            )
            .bind(&row.name)
            .bind(&row.facility)
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
                insert into "CommonMistake" (id, name, description, facility, "trainingTicketId")
                values ($1, $2, $3, $4, null)
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
            "common_mistakes",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_progressions(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceProgression>(
        r#"
        select id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor
        from training.training_progressions
        order by name asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let steps = sqlx::query_as::<_, SourceProgressionStep>(
        r#"select id, progression_id, lesson_id, sort_order, optional from training.training_progression_steps order by progression_id asc, sort_order asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in &rows {
        let business_key = format!("name:{}", row.name);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "training_progression", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "TrainingProgression" where name = $1"#,
            )
            .bind(&row.name)
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
                insert into "TrainingProgression" (id, name, "autoAssignNewHomeObs", "autoAssignNewVisitor")
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    name = excluded.name,
                    "autoAssignNewHomeObs" = excluded."autoAssignNewHomeObs",
                    "autoAssignNewVisitor" = excluded."autoAssignNewVisitor"
                "#,
            )
            .bind(&target_id)
            .bind(&row.name)
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
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row.name,
            )
            .await?;
        }
    }

    for row in &rows {
        if let Some(next_progression_id) = &row.next_progression_id {
            let target_id = target::find_mapping(&state.target, "training_progression", &row.id)
                .await?
                .context("missing progression mapping")?
                .target_id;
            let next_target_id =
                target::find_mapping(&state.target, "training_progression", next_progression_id)
                    .await?
                    .context("missing next progression mapping")?
                    .target_id;
            if !state.config.dry_run {
                sqlx::query(
                    r#"update "TrainingProgression" set "nextProgressionId" = $2 where id = $1"#,
                )
                .bind(&target_id)
                .bind(&next_target_id)
                .execute(&state.target)
                .await?;
            }
        }
    }

    for row in steps {
        let progression_target_id =
            target::find_mapping(&state.target, "training_progression", &row.progression_id)
                .await?
                .context("missing progression mapping for step")?
                .target_id;
        let lesson_target_id = target::find_mapping(&state.target, "lesson", &row.lesson_id)
            .await?
            .context("missing lesson mapping for step")?
            .target_id;
        let business_key = format!(
            "{progression_target_id}:{}:{}",
            row.sort_order, lesson_target_id
        );
        let target_id = target::find_mapping(&state.target, "training_progression_step", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "TrainingProgressionStep" (id, "order", "optional", "lessonId", "progressionId")
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    "order" = excluded."order",
                    "optional" = excluded."optional",
                    "lessonId" = excluded."lessonId",
                    "progressionId" = excluded."progressionId"
                "#,
            )
            .bind(&target_id)
            .bind(row.sort_order)
            .bind(row.optional)
            .bind(&lesson_target_id)
            .bind(&progression_target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "training_progression_step",
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
            "training_progressions",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_lesson_roster_changes(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceLessonRosterChange>(
        r#"select id, lesson_id, certification_type_id, certification_option, dossier_text from training.lesson_roster_changes"#,
    )
    .fetch_all(&state.source)
    .await?;
    for row in rows {
        let lesson_target_id = target::find_mapping(&state.target, "lesson", &row.lesson_id)
            .await?
            .context("missing lesson mapping for lesson roster change")?
            .target_id;
        let cert_target_id = target::find_mapping(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?
        .context("missing certification type mapping for lesson roster change")?
        .target_id;
        let option = normalize_certification_option(&row.certification_option)?.to_string();
        let business_key = format!("{lesson_target_id}:{cert_target_id}:{option}");
        let target_id = target::find_mapping(&state.target, "lesson_roster_change", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "LessonRosterChange" (id, "lessonId", "certificationTypeId", "certificationOption", "dossierText")
                values ($1, $2, $3, $4::"CertificationOption", $5)
                on conflict (id) do update set
                    "lessonId" = excluded."lessonId",
                    "certificationTypeId" = excluded."certificationTypeId",
                    "certificationOption" = excluded."certificationOption",
                    "dossierText" = excluded."dossierText"
                "#,
            )
            .bind(&target_id)
            .bind(&lesson_target_id)
            .bind(&cert_target_id)
            .bind(&option)
            .bind(&row.dossier_text)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "lesson_roster_change",
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
            "lesson_roster_changes",
        )
        .await?;
    }
    Ok(())
}
