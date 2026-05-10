use std::collections::HashMap;

use anyhow::{Result, bail};
use sqlx::{FromRow, Row};

use crate::{config::Domain, state::AppState, target};

const LIVE_ENVIRONMENT: &str = "live";

#[derive(Debug, Clone, FromRow)]
struct SourceControllerLog {
    id: String,
    user_id: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceControllerLogMonth {
    id: String,
    log_id: String,
    month: i32,
    year: i32,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    approach_hours: f64,
    center_hours: f64,
}

#[derive(Debug, Clone, FromRow)]
struct TargetRollupRow {
    cid: i64,
    year: i32,
    month: i32,
    online_seconds: i64,
    delivery_seconds: i64,
    ground_seconds: i64,
    tower_seconds: i64,
    tracon_seconds: i64,
    center_seconds: i64,
}

pub async fn run(state: &mut AppState) -> Result<()> {
    verify_counts(state).await?;
    verify_referential_integrity(state).await?;
    verify_semantics(state).await?;
    Ok(())
}

async fn verify_counts(state: &mut AppState) -> Result<()> {
    let mut checks = Vec::new();

    if domain_enabled(state, Domain::Users) {
        checks.push((
            "users",
            r#"select count(*) from public."User""#,
            r#"select count(*) from identity.users"#,
        ));
    }
    if domain_enabled(state, Domain::Org) {
        checks.extend([
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
        ]);
    }
    if domain_enabled(state, Domain::Training) {
        checks.extend([
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
        ]);
    }
    if domain_enabled(state, Domain::Feedback) {
        checks.extend([
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
        ]);
    }
    if domain_enabled(state, Domain::Events) {
        checks.extend([
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
        ]);
    }

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

    if domain_enabled(state, Domain::Stats) {
        verify_stats_counts(state).await?;
    }

    Ok(())
}

async fn verify_referential_integrity(state: &mut AppState) -> Result<()> {
    if domain_enabled(state, Domain::Events) {
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
    }

    if domain_enabled(state, Domain::Training) {
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
    }

    if domain_enabled(state, Domain::Org) {
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

    if domain_enabled(state, Domain::Stats) {
        verify_stats_referential_integrity(state).await?;
    }

    Ok(())
}

async fn verify_semantics(state: &mut AppState) -> Result<()> {
    if domain_enabled(state, Domain::Users) {
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
    }

    if domain_enabled(state, Domain::Training) {
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
    }

    if domain_enabled(state, Domain::Events) {
        let bad_event_ranges: i64 =
            sqlx::query_scalar(r#"select count(*) from events.events where ends_at < starts_at"#)
                .fetch_one(&state.target)
                .await?;
        if bad_event_ranges > 0 {
            bail!("target contains events with invalid date ranges");
        }
    }

    if domain_enabled(state, Domain::Stats) {
        verify_stats_values(state).await?;
    }

    let domain = state.report.domain_mut("verify");
    domain.planned += 1;
    domain.updated += 1;

    let _ = sqlx::query("select 1")
        .fetch_one(&state.target)
        .await?
        .get::<i32, _>(0);
    Ok(())
}

async fn verify_stats_counts(state: &mut AppState) -> Result<()> {
    let expected = expected_stats_rollups(state).await?;
    let target_count: i64 = sqlx::query_scalar(
        r#"select count(*) from stats.controller_monthly_rollups where environment = $1"#,
    )
    .bind(LIVE_ENVIRONMENT)
    .fetch_one(&state.target)
    .await?;

    if target_count != expected.len() as i64 {
        bail!(
            "stats rollup count mismatch for live environment (target {} != expected {})",
            target_count,
            expected.len()
        );
    }

    Ok(())
}

async fn verify_stats_referential_integrity(state: &mut AppState) -> Result<()> {
    let duplicate_rollups: i64 = sqlx::query_scalar(
        r#"
        select count(*) from (
            select cid, year, month
            from stats.controller_monthly_rollups
            where environment = $1
            group by cid, year, month
            having count(*) > 1
        ) d
        "#,
    )
    .bind(LIVE_ENVIRONMENT)
    .fetch_one(&state.target)
    .await?;
    if duplicate_rollups > 0 {
        bail!("stats monthly rollups contain duplicate live cid/year/month rows");
    }

    Ok(())
}

async fn verify_stats_values(state: &mut AppState) -> Result<()> {
    let expected = expected_stats_rollups(state).await?;
    let actual_rows = sqlx::query_as::<_, TargetRollupRow>(
        r#"
        select
            cid,
            year,
            month,
            online_seconds,
            delivery_seconds,
            ground_seconds,
            tower_seconds,
            tracon_seconds,
            center_seconds
        from stats.controller_monthly_rollups
        where environment = $1
        "#,
    )
    .bind(LIVE_ENVIRONMENT)
    .fetch_all(&state.target)
    .await?;
    let actual = actual_rows
        .into_iter()
        .map(|row| ((row.cid, row.year, row.month), row))
        .collect::<HashMap<_, _>>();

    for (key, expected_row) in expected {
        let Some(actual_row) = actual.get(&key) else {
            bail!(
                "missing stats rollup for live:{}:{}:{}",
                key.0,
                key.1,
                key.2
            );
        };

        for (name, lhs, rhs) in [
            (
                "delivery_seconds",
                actual_row.delivery_seconds,
                expected_row.delivery_seconds,
            ),
            (
                "ground_seconds",
                actual_row.ground_seconds,
                expected_row.ground_seconds,
            ),
            (
                "tower_seconds",
                actual_row.tower_seconds,
                expected_row.tower_seconds,
            ),
            (
                "tracon_seconds",
                actual_row.tracon_seconds,
                expected_row.tracon_seconds,
            ),
            (
                "center_seconds",
                actual_row.center_seconds,
                expected_row.center_seconds,
            ),
            (
                "online_seconds",
                actual_row.online_seconds,
                expected_row.online_seconds,
            ),
        ] {
            if (lhs - rhs).abs() > 1 {
                bail!(
                    "stats rollup value mismatch for live:{}:{}:{} field {} (actual {} != expected {})",
                    key.0,
                    key.1,
                    key.2,
                    name,
                    lhs,
                    rhs
                );
            }
        }
    }

    Ok(())
}

async fn expected_stats_rollups(
    state: &mut AppState,
) -> Result<HashMap<(i64, i32, i32), TargetRollupRow>> {
    let logs = sqlx::query_as::<_, SourceControllerLog>(
        r#"
        select id, "userId" as user_id
        from public."ControllerLog"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let months = sqlx::query_as::<_, SourceControllerLogMonth>(
        r#"
        select
            id,
            "logId" as log_id,
            month,
            year,
            "deliveryHours" as delivery_hours,
            "groundHours" as ground_hours,
            "towerHours" as tower_hours,
            "approachHours" as approach_hours,
            "centerHours" as center_hours
        from public."ControllerLogMonth"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    let mut log_to_cid = HashMap::<String, i64>::new();
    for log in logs {
        let Some(target_user_id) = resolve_target_user_id(state, &log.user_id).await? else {
            continue;
        };
        let Some(cid) =
            sqlx::query_scalar::<_, i64>(r#"select cid from identity.users where id = $1"#)
                .bind(&target_user_id)
                .fetch_optional(&state.target)
                .await?
        else {
            continue;
        };
        log_to_cid.insert(log.id, cid);
    }

    let mut expected = HashMap::new();
    for row in months {
        let Some(&cid) = log_to_cid.get(&row.log_id) else {
            continue;
        };
        let delivery_seconds = hours_to_seconds(row.delivery_hours)?;
        let ground_seconds = hours_to_seconds(row.ground_hours)?;
        let tower_seconds = hours_to_seconds(row.tower_hours)?;
        let tracon_seconds = hours_to_seconds(row.approach_hours)?;
        let center_seconds = hours_to_seconds(row.center_hours)?;
        let online_seconds =
            delivery_seconds + ground_seconds + tower_seconds + tracon_seconds + center_seconds;

        expected.insert(
            (cid, row.year, row.month),
            TargetRollupRow {
                cid,
                year: row.year,
                month: row.month,
                online_seconds,
                delivery_seconds,
                ground_seconds,
                tower_seconds,
                tracon_seconds,
                center_seconds,
            },
        );
    }

    Ok(expected)
}

async fn resolve_target_user_id(
    state: &mut AppState,
    source_user_id: &str,
) -> Result<Option<String>> {
    if let Some(mapping) = target::find_mapping(&state.target, "user", source_user_id).await? {
        return Ok(Some(mapping.target_id));
    }

    let exists = sqlx::query_scalar::<_, bool>(
        r#"select exists(select 1 from identity.users where id = $1)"#,
    )
    .bind(source_user_id)
    .fetch_one(&state.target)
    .await?;
    if exists {
        Ok(Some(source_user_id.to_string()))
    } else {
        Ok(None)
    }
}

fn hours_to_seconds(hours: f64) -> Result<i64> {
    if !hours.is_finite() {
        bail!("stats hour value is not finite");
    }
    if hours < 0.0 {
        bail!("stats hour value is negative");
    }
    Ok((hours * 3600.0).round() as i64)
}

fn domain_enabled(state: &AppState, domain: Domain) -> bool {
    state.config.domains.contains(&domain)
}
