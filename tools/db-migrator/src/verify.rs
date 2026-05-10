use anyhow::{Result, bail};
use sqlx::Row;

use crate::state::AppState;

pub async fn run(state: &mut AppState) -> Result<()> {
    verify_counts(state).await?;
    verify_referential_integrity(state).await?;
    verify_semantics(state).await?;
    Ok(())
}

async fn verify_counts(state: &mut AppState) -> Result<()> {
    let checks = [
        (
            "users",
            r#"select count(*) from public."User""#,
            r#"select count(*) from identity.users"#,
        ),
        (
            "visitor_applications",
            r#"select count(*) from public."VisitorApplication""#,
            r#"select count(*) from org.visitor_applications"#,
        ),
        (
            "loas",
            r#"select count(*) from public."LOA""#,
            r#"select count(*) from org.loas"#,
        ),
        (
            "certifications",
            r#"select count(*) from public."Certification""#,
            r#"select count(*) from org.user_certifications"#,
        ),
        (
            "solo_certifications",
            r#"select count(*) from public."SoloCertification""#,
            r#"select count(*) from org.user_solo_certifications"#,
        ),
        (
            "training_assignments",
            r#"select count(*) from public."TrainingAssignment""#,
            r#"select count(*) from training.training_assignments"#,
        ),
        (
            "training_requests",
            r#"select count(*) from public."TrainingAssignmentRequest""#,
            r#"select count(*) from training.training_assignment_requests"#,
        ),
        (
            "release_requests",
            r#"select count(*) from public."TrainerReleaseRequest""#,
            r#"select count(*) from training.trainer_release_requests"#,
        ),
        (
            "appointments",
            r#"select count(*) from public."TrainingAppointment""#,
            r#"select count(*) from training.training_appointments"#,
        ),
        (
            "ots_recommendations",
            r#"select count(*) from public."OtsRecommendation""#,
            r#"select count(*) from training.ots_recommendations"#,
        ),
        (
            "feedback",
            r#"select count(*) from public."Feedback""#,
            r#"select count(*) from feedback.feedback_items"#,
        ),
        (
            "dossier_entries",
            r#"select count(*) from public."DossierEntry""#,
            r#"select count(*) from feedback.dossier_entries"#,
        ),
        (
            "incidents",
            r#"select count(*) from public."IncidentReport""#,
            r#"select count(*) from feedback.incident_reports"#,
        ),
        (
            "events",
            r#"select count(*) from public."Event""#,
            r#"select count(*) from events.events"#,
        ),
        (
            "event_positions",
            r#"select count(*) from public."EventPosition""#,
            r#"select count(*) from events.event_positions"#,
        ),
        (
            "event_tmis",
            r#"select count(*) from public."EventTmi""#,
            r#"select count(*) from events.event_tmis"#,
        ),
        (
            "event_presets",
            r#"select count(*) from public."EventPositionPreset""#,
            r#"select count(*) from events.event_position_presets"#,
        ),
    ];

    for (name, source_query, target_query) in checks {
        let source_count: i64 = sqlx::query_scalar(source_query)
            .fetch_one(&state.source)
            .await?;
        let target_count: i64 = sqlx::query_scalar(target_query)
            .fetch_one(&state.target)
            .await?;
        if target_count > source_count {
            bail!("target count for {name} exceeds source count ({target_count} > {source_count})");
        }
    }

    Ok(())
}

async fn verify_referential_integrity(state: &mut AppState) -> Result<()> {
    let orphan_event_positions: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from events.event_positions ep
        left join events.events e on e.id = ep.event_id
        left join identity.users u on u.id = ep.user_id
        where e.id is null or (ep.user_id is not null and u.id is null)
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if orphan_event_positions > 0 {
        bail!(
            "event position referential check failed with {orphan_event_positions} orphaned rows"
        );
    }

    let orphan_tickets: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from training.training_tickets tt
        left join training.training_sessions ts on ts.id = tt.session_id
        left join training.lessons l on l.id = tt.lesson_id
        where ts.id is null or l.id is null
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if orphan_tickets > 0 {
        bail!("training ticket referential check failed with {orphan_tickets} orphaned rows");
    }

    let orphan_certs: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from org.user_certifications uc
        left join identity.users u on u.id = uc.user_id
        left join org.certification_types ct on ct.id = uc.certification_type_id
        where u.id is null or ct.id is null
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if orphan_certs > 0 {
        bail!("user certification referential check failed with {orphan_certs} orphaned rows");
    }

    let missing_maps: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from migrator.migration_entity_map
        where target_id is null or target_id = ''
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if missing_maps > 0 {
        bail!("migration entity map contains {missing_maps} invalid target ids");
    }

    Ok(())
}

async fn verify_semantics(state: &mut AppState) -> Result<()> {
    let duplicate_cids: i64 = sqlx::query_scalar(
        r#"
        select count(*) from (
            select cid
            from identity.users
            where cid is not null
            group by cid
            having count(*) > 1
        ) d
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if duplicate_cids > 0 {
        bail!("target contains duplicate user CIDs");
    }

    let duplicate_assignments: i64 = sqlx::query_scalar(
        r#"
        select count(*) from (
            select student_id
            from training.training_assignments
            group by student_id
            having count(*) > 1
        ) d
        "#,
    )
    .fetch_one(&state.target)
    .await?;
    if duplicate_assignments > 0 {
        bail!("target contains duplicate training assignments by student");
    }

    let bad_event_ranges: i64 =
        sqlx::query_scalar(r#"select count(*) from events.events where ends_at < starts_at"#)
            .fetch_one(&state.target)
            .await?;
    if bad_event_ranges > 0 {
        bail!("target contains events with invalid date ranges");
    }

    let domain = state.report.domain_mut("verify");
    domain.planned += 3;
    domain.updated += 3;

    let _ = sqlx::query("select 1")
        .fetch_one(&state.target)
        .await?
        .get::<i32, _>(0);
    Ok(())
}
