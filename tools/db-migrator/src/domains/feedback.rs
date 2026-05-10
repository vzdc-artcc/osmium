use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, assume_utc_opt},
    state::AppState,
    target,
};

const DOMAIN: &str = "feedback";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceFeedback {
    id: String,
    pilot_id: String,
    controller_id: String,
    pilot_callsign: String,
    controller_position: String,
    rating: i32,
    comments: Option<String>,
    staff_comments: Option<String>,
    status: String,
    submitted_at: NaiveDateTime,
    decided_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceDossierEntry {
    id: String,
    user_id: String,
    writer_id: String,
    message: String,
    timestamp: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceIncident {
    id: String,
    reporter_id: String,
    reportee_id: String,
    timestamp: NaiveDateTime,
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
        select
            id,
            "pilotId" as pilot_id,
            "controllerId" as controller_id,
            "pilotCallsign" as pilot_callsign,
            "controllerPosition" as controller_position,
            rating,
            comments,
            "staffComments" as staff_comments,
            status::text as status,
            "submittedAt" as submitted_at,
            "decidedAt" as decided_at
        from public."Feedback"
        order by "submittedAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let submitter_user_id = mapped_id(&state.target, "user", &row.pilot_id).await?;
        let target_user_id = mapped_id(&state.target, "user", &row.controller_id).await?;
        let target_id = mapped_or_same(&state.target, "feedback_item", &row.id).await?;
        let existed = exists(&state.target, "feedback.feedback_items", &target_id).await?;
        let source_business_key = format!(
            "{submitter_user_id}:{target_user_id}:{}:{}",
            row.submitted_at, row.controller_position
        );

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into feedback.feedback_items (
                    id, submitter_user_id, target_user_id, pilot_callsign, controller_position,
                    rating, comments, staff_comments, status, submitted_at, decided_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                on conflict (id) do update set
                    submitter_user_id = excluded.submitter_user_id,
                    target_user_id = excluded.target_user_id,
                    pilot_callsign = excluded.pilot_callsign,
                    controller_position = excluded.controller_position,
                    rating = excluded.rating,
                    comments = excluded.comments,
                    staff_comments = excluded.staff_comments,
                    status = excluded.status,
                    submitted_at = excluded.submitted_at,
                    decided_at = excluded.decided_at
                "#,
            )
            .bind(&target_id)
            .bind(&submitter_user_id)
            .bind(&target_user_id)
            .bind(&row.pilot_callsign)
            .bind(&row.controller_position)
            .bind(row.rating)
            .bind(&row.comments)
            .bind(&row.staff_comments)
            .bind(&row.status)
            .bind(assume_utc(row.submitted_at))
            .bind(assume_utc_opt(row.decided_at))
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "feedback_item",
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

    checkpoint(state, "feedback_items").await
}

async fn migrate_dossier_entries(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceDossierEntry>(
        r#"
        select
            id,
            "userId" as user_id,
            "writerId" as writer_id,
            message,
            timestamp
        from public."DossierEntry"
        order by timestamp asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let writer_id = mapped_id(&state.target, "user", &row.writer_id).await?;
        let target_id = mapped_or_same(&state.target, "dossier_entry", &row.id).await?;
        let existed = exists(&state.target, "feedback.dossier_entries", &target_id).await?;
        let source_business_key =
            format!("{user_id}:{writer_id}:{}:{}", row.timestamp, row.message);

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into feedback.dossier_entries (id, user_id, writer_id, message, timestamp)
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    writer_id = excluded.writer_id,
                    message = excluded.message,
                    timestamp = excluded.timestamp
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(&writer_id)
            .bind(&row.message)
            .bind(assume_utc(row.timestamp))
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "dossier_entry",
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

    checkpoint(state, "dossier_entries").await
}

async fn migrate_incidents(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceIncident>(
        r#"
        select
            id,
            "reporterId" as reporter_id,
            "reporteeId" as reportee_id,
            timestamp,
            reason,
            closed,
            "reporterCallsign" as reporter_callsign,
            "reporteeCallsign" as reportee_callsign
        from public."IncidentReport"
        order by timestamp asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let reporter_id = mapped_id(&state.target, "user", &row.reporter_id).await?;
        let reportee_id = mapped_id(&state.target, "user", &row.reportee_id).await?;
        let target_id = mapped_or_same(&state.target, "incident", &row.id).await?;
        let existed = exists(&state.target, "feedback.incident_reports", &target_id).await?;
        let source_business_key = format!(
            "{reporter_id}:{reportee_id}:{}:{}",
            row.timestamp, row.reason
        );

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into feedback.incident_reports (
                    id, reporter_id, reportee_id, timestamp, reason, closed, reporter_callsign, reportee_callsign
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    reporter_id = excluded.reporter_id,
                    reportee_id = excluded.reportee_id,
                    timestamp = excluded.timestamp,
                    reason = excluded.reason,
                    closed = excluded.closed,
                    reporter_callsign = excluded.reporter_callsign,
                    reportee_callsign = excluded.reportee_callsign
                "#,
            )
            .bind(&target_id)
            .bind(&reporter_id)
            .bind(&reportee_id)
            .bind(assume_utc(row.timestamp))
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

    checkpoint(state, "incident_reports").await
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
