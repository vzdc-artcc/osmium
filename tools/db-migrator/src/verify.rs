use anyhow::{Result, bail};
use sqlx::Row;

use crate::{report::ReportIssue, state::AppState};

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
            "select count(*) from identity.users",
            "select count(*) from \"User\"",
        ),
        (
            "visitor_applications",
            "select count(*) from org.visitor_applications",
            "select count(*) from \"VisitorApplication\"",
        ),
        (
            "loas",
            "select count(*) from org.loas",
            "select count(*) from \"LOA\"",
        ),
        (
            "certifications",
            "select count(*) from org.user_certifications",
            "select count(*) from \"Certification\"",
        ),
        (
            "solo_certifications",
            "select count(*) from org.user_solo_certifications",
            "select count(*) from \"SoloCertification\"",
        ),
        (
            "training_assignments",
            "select count(*) from training.training_assignments",
            "select count(*) from \"TrainingAssignment\"",
        ),
        (
            "training_requests",
            "select count(*) from training.training_assignment_requests where status = 'PENDING'",
            "select count(*) from \"TrainingAssignmentRequest\"",
        ),
        (
            "release_requests",
            "select count(*) from training.trainer_release_requests where status = 'PENDING'",
            "select count(*) from \"TrainerReleaseRequest\"",
        ),
        (
            "appointments",
            "select count(*) from training.training_appointments",
            "select count(*) from \"TrainingAppointment\"",
        ),
        (
            "ots_recommendations",
            "select count(*) from training.ots_recommendations",
            "select count(*) from \"OtsRecommendation\"",
        ),
        (
            "feedback",
            "select count(*) from feedback.feedback_items",
            "select count(*) from \"Feedback\"",
        ),
        (
            "dossier_entries",
            "select count(*) from feedback.dossier_entries",
            "select count(*) from \"DossierEntry\"",
        ),
        (
            "incidents",
            "select count(*) from feedback.incident_reports",
            "select count(*) from \"IncidentReport\"",
        ),
        (
            "events",
            "select count(*) from events.events",
            "select count(*) from \"Event\"",
        ),
        (
            "event_positions",
            "select count(*) from events.event_positions",
            "select count(*) from \"EventPosition\"",
        ),
        (
            "event_tmis",
            "select count(*) from events.event_tmis",
            "select count(*) from \"EventTmi\"",
        ),
        (
            "event_presets",
            "select count(*) from events.event_position_presets",
            "select count(*) from \"EventPositionPreset\"",
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
    let orphans: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from "EventPosition" ep
        left join "Event" e on e.id = ep."eventId"
        left join "User" u on u.id = ep."userId"
        where e.id is null or (ep."userId" is not null and u.id is null)
        "#,
    )
    .fetch_one(&state.target)
    .await?;

    if orphans > 0 {
        bail!("event position referential check failed with {orphans} orphaned rows");
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
            select "cid"
            from "User"
            group by "cid"
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
            select "studentId"
            from "TrainingAssignment"
            group by "studentId"
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
        sqlx::query_scalar(r#"select count(*) from "Event" where "end" < "start""#)
            .fetch_one(&state.target)
            .await?;
    if bad_event_ranges > 0 {
        bail!("target contains events with invalid date ranges");
    }

    let domain = state.report.domain_mut("verify");
    domain.planned += 3;
    domain.updated += 3;

    if !state.report.errors.is_empty() {
        let summary = ReportIssue {
            domain: "verify".to_string(),
            entity_type: "summary".to_string(),
            source_id: state.report.run_id.clone(),
            message: "verification finished with prior migration errors".to_string(),
        };
        let _ = summary;
    }

    let _ = sqlx::query("select 1")
        .fetch_one(&state.target)
        .await?
        .get::<i32, _>(0);
    Ok(())
}
