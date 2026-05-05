use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{helpers::new_id, state::AppState, target};

const DOMAIN: &str = "feedback";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceFeedback {
    id: String,
    submitter_user_id: String,
    target_user_id: String,
    pilot_callsign: String,
    controller_position: String,
    rating: i32,
    comments: Option<String>,
    staff_comments: Option<String>,
    status: String,
    submitted_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceDossierEntry {
    id: String,
    user_id: String,
    writer_id: String,
    message: String,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceIncident {
    id: String,
    reporter_id: String,
    reportee_id: String,
    timestamp: DateTime<Utc>,
    reason: String,
    closed: bool,
    reporter_callsign: Option<String>,
    reportee_callsign: Option<String>,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    migrate_feedback_items(state).await?;
    migrate_dossier_entries(state).await?;
    migrate_incidents(state).await?;
    Ok(())
}

async fn migrate_feedback_items(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceFeedback>(
        r#"
        select id, submitter_user_id, target_user_id, pilot_callsign, controller_position, rating, comments,
               staff_comments, status, submitted_at, decided_at
        from feedback.feedback_items
        order by submitted_at asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let pilot_id = mapped_id(&state.target, "user", &row.submitter_user_id).await?;
        let controller_id = mapped_id(&state.target, "user", &row.target_user_id).await?;
        let business_key = format!(
            "{pilot_id}:{controller_id}:{}:{}",
            row.submitted_at.to_rfc3339(),
            row.controller_position
        );
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "feedback_item", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "Feedback" where "pilotId" = $1 and "controllerId" = $2 and "submittedAt" = $3 and "controllerPosition" = $4"#,
            )
            .bind(&pilot_id)
            .bind(&controller_id)
            .bind(row.submitted_at)
            .bind(&row.controller_position)
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
                insert into "Feedback" (
                    id, "pilotId", "pilotCallsign", "controllerId", "controllerPosition", rating,
                    comments, "staffComments", status, "submittedAt", "decidedAt"
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9::"FeedbackStatus", $10, $11)
                on conflict (id) do update set
                    "pilotId" = excluded."pilotId",
                    "pilotCallsign" = excluded."pilotCallsign",
                    "controllerId" = excluded."controllerId",
                    "controllerPosition" = excluded."controllerPosition",
                    rating = excluded.rating,
                    comments = excluded.comments,
                    "staffComments" = excluded."staffComments",
                    status = excluded.status,
                    "submittedAt" = excluded."submittedAt",
                    "decidedAt" = excluded."decidedAt"
                "#,
            )
            .bind(&target_id)
            .bind(&pilot_id)
            .bind(&row.pilot_callsign)
            .bind(&controller_id)
            .bind(&row.controller_position)
            .bind(row.rating)
            .bind(&row.comments)
            .bind(&row.staff_comments)
            .bind(&row.status)
            .bind(row.submitted_at)
            .bind(row.decided_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "feedback_item",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
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
            "feedback_items",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_dossier_entries(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceDossierEntry>(
        r#"select id, user_id, writer_id, message, timestamp from feedback.dossier_entries order by timestamp asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let writer_id = mapped_id(&state.target, "user", &row.writer_id).await?;
        let business_key = format!(
            "{user_id}:{writer_id}:{}:{}",
            row.timestamp.to_rfc3339(),
            row.message
        );
        let target_id = target::find_mapping(&state.target, "dossier_entry", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "DossierEntry" (id, "userId", "writerId", message, timestamp)
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    "userId" = excluded."userId",
                    "writerId" = excluded."writerId",
                    message = excluded.message,
                    timestamp = excluded.timestamp
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(&writer_id)
            .bind(&row.message)
            .bind(row.timestamp)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "dossier_entry",
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
            "dossier_entries",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_incidents(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceIncident>(
        r#"
        select id, reporter_id, reportee_id, timestamp, reason, closed, reporter_callsign, reportee_callsign
        from feedback.incident_reports
        order by timestamp asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let reporter_id = mapped_id(&state.target, "user", &row.reporter_id).await?;
        let reportee_id = mapped_id(&state.target, "user", &row.reportee_id).await?;
        let business_key = format!(
            "{reporter_id}:{reportee_id}:{}:{}",
            row.timestamp.to_rfc3339(),
            row.reason
        );
        let target_id = target::find_mapping(&state.target, "incident", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "IncidentReport" (
                    id, "reporterId", "reporteeId", timestamp, reason, closed, "reporterCallsign", "reporteeCallsign"
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    "reporterId" = excluded."reporterId",
                    "reporteeId" = excluded."reporteeId",
                    timestamp = excluded.timestamp,
                    reason = excluded.reason,
                    closed = excluded.closed,
                    "reporterCallsign" = excluded."reporterCallsign",
                    "reporteeCallsign" = excluded."reporteeCallsign"
                "#,
            )
            .bind(&target_id)
            .bind(&reporter_id)
            .bind(&reportee_id)
            .bind(row.timestamp)
            .bind(&row.reason)
            .bind(row.closed)
            .bind(&row.reporter_callsign)
            .bind(&row.reportee_callsign)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "incident",
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
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "incidents").await?;
    }
    Ok(())
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .with_context(|| format!("missing mapping for {entity_type}/{source_id}"))?
        .target_id)
}
