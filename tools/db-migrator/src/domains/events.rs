use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{merge_note, new_id, record_warning},
    mapping::normalize_event_type,
    state::AppState,
    target,
};

const DOMAIN: &str = "events";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceEvent {
    id: String,
    title: String,
    event_type: String,
    host: Option<String>,
    description: Option<String>,
    hidden: bool,
    positions_locked: bool,
    manual_positions_open: bool,
    archived_at: Option<DateTime<Utc>>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    featured_fields: Vec<String>,
    preset_positions: Vec<String>,
    enable_buffer_times: bool,
    featured_field_configs: Option<serde_json::Value>,
    tmis: Option<String>,
    ops_free_text: Option<String>,
    ops_plan_published: bool,
    ops_planner_id: Option<String>,
    banner_asset_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceEventPosition {
    id: String,
    event_id: String,
    callsign: String,
    user_id: Option<String>,
    requested_position: Option<String>,
    requested_secondary_position: String,
    notes: Option<String>,
    requested_start_time: Option<DateTime<Utc>>,
    requested_end_time: Option<DateTime<Utc>>,
    final_start_time: Option<DateTime<Utc>>,
    final_end_time: Option<DateTime<Utc>>,
    final_position: Option<String>,
    final_notes: Option<String>,
    controlling_category: Option<String>,
    is_instructor: bool,
    is_solo: bool,
    is_ots: bool,
    is_tmu: bool,
    is_cic: bool,
    published: bool,
    submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceEventTmi {
    id: String,
    event_id: String,
    tmi_type: String,
    start_time: DateTime<Utc>,
    notes: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceOpsPlanFile {
    id: String,
    event_id: String,
    asset_id: Option<String>,
    filename: String,
    url: Option<String>,
    file_type: Option<String>,
    uploaded_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    let events = sqlx::query_as::<_, SourceEvent>(
        r#"
        select id, title, type as event_type, host, description, hidden, positions_locked, manual_positions_open,
               archived_at, starts_at, ends_at, featured_fields, preset_positions, enable_buffer_times,
               featured_field_configs, tmis, ops_free_text, ops_plan_published, ops_planner_id, banner_asset_id
        from events.events
        order by starts_at asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let positions = sqlx::query_as::<_, SourceEventPosition>(
        r#"
        select id, event_id, callsign, user_id, requested_position, requested_secondary_position, notes,
               requested_start_time, requested_end_time, final_start_time, final_end_time, final_position,
               final_notes, controlling_category, is_instructor, is_solo, is_ots, is_tmu, is_cic,
               published, submitted_at
        from events.event_positions
        order by submitted_at asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let tmis = sqlx::query_as::<_, SourceEventTmi>(
        r#"select id, event_id, tmi_type, start_time, notes from events.event_tmis order by start_time asc"#,
    )
    .fetch_all(&state.source)
    .await?;
    let files = sqlx::query_as::<_, SourceOpsPlanFile>(
        r#"select id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at from events.ops_plan_files"#,
    )
    .fetch_all(&state.source)
    .await?;

    let events_by_id = events
        .iter()
        .map(|row| (row.id.clone(), row.clone()))
        .collect::<HashMap<_, _>>();

    for row in events {
        state.report.domain_mut(DOMAIN).planned += 1;
        let event_type = normalize_event_type(&row.event_type)?.to_string();
        let business_key = format!(
            "{}:{}:{}",
            row.title,
            row.starts_at.to_rfc3339(),
            row.ends_at.to_rfc3339()
        );
        let mut target_id =
            if let Some(mapping) = target::find_mapping(&state.target, "event", &row.id).await? {
                Some(mapping.target_id)
            } else {
                sqlx::query_scalar::<_, String>(
                    r#"select id from "Event" where name = $1 and start = $2 and "end" = $3"#,
                )
                .bind(&row.title)
                .bind(row.starts_at)
                .bind(row.ends_at)
                .fetch_optional(&state.target)
                .await?
            };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        let ops_planner_id = match &row.ops_planner_id {
            Some(source_user_id) => target::find_mapping(&state.target, "user", source_user_id)
                .await?
                .map(|row| row.target_id),
            None => None,
        };
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "Event" (
                    id, name, type, host, "featuredFields", description, "bannerKey", hidden, "positionsLocked",
                    "manualPositionsOpen", archived, start, "end", "presetPositions", "enableBufferTimes",
                    "featuredFieldConfigs", tmis, "opsFreeText", "opsPlanPublished", "opsPlannerId"
                )
                values (
                    $1, $2, $3::"EventType", $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                    $14, $15, $16, $17, $18, $19, $20
                )
                on conflict (id) do update set
                    name = excluded.name,
                    type = excluded.type,
                    host = excluded.host,
                    "featuredFields" = excluded."featuredFields",
                    description = excluded.description,
                    "bannerKey" = excluded."bannerKey",
                    hidden = excluded.hidden,
                    "positionsLocked" = excluded."positionsLocked",
                    "manualPositionsOpen" = excluded."manualPositionsOpen",
                    archived = excluded.archived,
                    start = excluded.start,
                    "end" = excluded."end",
                    "presetPositions" = excluded."presetPositions",
                    "enableBufferTimes" = excluded."enableBufferTimes",
                    "featuredFieldConfigs" = excluded."featuredFieldConfigs",
                    tmis = excluded.tmis,
                    "opsFreeText" = excluded."opsFreeText",
                    "opsPlanPublished" = excluded."opsPlanPublished",
                    "opsPlannerId" = excluded."opsPlannerId"
                "#,
            )
            .bind(&target_id)
            .bind(&row.title)
            .bind(&event_type)
            .bind(row.host.as_deref().unwrap_or("ERR - CTC EVENTS STAFF"))
            .bind(&row.featured_fields)
            .bind(row.description.as_deref().unwrap_or(""))
            .bind(&row.banner_asset_id)
            .bind(row.hidden)
            .bind(row.positions_locked)
            .bind(row.manual_positions_open)
            .bind(row.archived_at)
            .bind(row.starts_at)
            .bind(row.ends_at)
            .bind(&row.preset_positions)
            .bind(row.enable_buffer_times)
            .bind(&row.featured_field_configs)
            .bind(&row.tmis)
            .bind(&row.ops_free_text)
            .bind(row.ops_plan_published)
            .bind(&ops_planner_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event",
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

    for row in positions {
        let event_target_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let event = events_by_id
            .get(&row.event_id)
            .with_context(|| format!("missing source event for position {}", row.id))?;
        let requested_position = row
            .requested_position
            .clone()
            .unwrap_or_else(|| row.callsign.clone());
        let requested_start = row.requested_start_time.unwrap_or(event.starts_at);
        let requested_end = row.requested_end_time.unwrap_or(event.ends_at);
        let mut notes = row.notes.clone();
        let user_target_id = match &row.user_id {
            Some(source_user_id) => target::find_mapping(&state.target, "user", source_user_id)
                .await?
                .map(|row| row.target_id),
            None => None,
        };
        if row.user_id.is_none() {
            notes = Some(merge_note(
                &format!("[legacy callsign: {}]", row.callsign),
                notes.as_deref(),
            ));
            record_warning(
                state,
                DOMAIN,
                "event_position",
                &row.id,
                "event position had no user_id; callsign was preserved inside notes",
            )
            .await?;
        }
        let business_key = if let Some(user_target_id) = &user_target_id {
            format!("{event_target_id}:{user_target_id}")
        } else {
            format!("{event_target_id}:callsign:{}", row.callsign)
        };
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "event_position", &row.id).await?
        {
            Some(mapping.target_id)
        } else if let Some(user_target_id) = &user_target_id {
            sqlx::query_scalar::<_, String>(
                r#"select id from "EventPosition" where "eventId" = $1 and "userId" = $2"#,
            )
            .bind(&event_target_id)
            .bind(user_target_id)
            .fetch_optional(&state.target)
            .await?
        } else {
            None
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "EventPosition" (
                    id, "eventId", "userId", "requestedPosition", "requestedSecondaryPosition", notes,
                    "requestedStartTime", "requestedEndTime", "finalStartTime", "finalEndTime", "finalPosition",
                    "finalNotes", "controllingCategory", "isInstructor", "isSolo", "isOts", "isTmu", "isCic",
                    published, "submittedAt"
                )
                values (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::"ControllingCategory",
                    $14, $15, $16, $17, $18, $19, $20
                )
                on conflict (id) do update set
                    "eventId" = excluded."eventId",
                    "userId" = excluded."userId",
                    "requestedPosition" = excluded."requestedPosition",
                    "requestedSecondaryPosition" = excluded."requestedSecondaryPosition",
                    notes = excluded.notes,
                    "requestedStartTime" = excluded."requestedStartTime",
                    "requestedEndTime" = excluded."requestedEndTime",
                    "finalStartTime" = excluded."finalStartTime",
                    "finalEndTime" = excluded."finalEndTime",
                    "finalPosition" = excluded."finalPosition",
                    "finalNotes" = excluded."finalNotes",
                    "controllingCategory" = excluded."controllingCategory",
                    "isInstructor" = excluded."isInstructor",
                    "isSolo" = excluded."isSolo",
                    "isOts" = excluded."isOts",
                    "isTmu" = excluded."isTmu",
                    "isCic" = excluded."isCic",
                    published = excluded.published,
                    "submittedAt" = excluded."submittedAt"
                "#,
            )
            .bind(&target_id)
            .bind(&event_target_id)
            .bind(&user_target_id)
            .bind(&requested_position)
            .bind(&row.requested_secondary_position)
            .bind(&notes)
            .bind(requested_start)
            .bind(requested_end)
            .bind(row.final_start_time)
            .bind(row.final_end_time)
            .bind(&row.final_position)
            .bind(&row.final_notes)
            .bind(&row.controlling_category)
            .bind(row.is_instructor)
            .bind(row.is_solo)
            .bind(row.is_ots)
            .bind(row.is_tmu)
            .bind(row.is_cic)
            .bind(row.published)
            .bind(row.submitted_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event_position",
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

    for row in tmis {
        let event_target_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let category = crate::mapping::normalize_tmi_category(&row.tmi_type)?.to_string();
        let text = match row
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(note) => format!("[{}] {}", row.start_time.format("%Y-%m-%d %H:%MZ"), note),
            None => format!("[{}] legacy TMI", row.start_time.format("%Y-%m-%d %H:%MZ")),
        };
        let business_key = format!(
            "{event_target_id}:{}:{}:{}",
            row.start_time.to_rfc3339(),
            category,
            text
        );
        let target_id = target::find_mapping(&state.target, "event_tmi", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "EventTmi" (id, "eventId", category, text, "createdAt")
                values ($1, $2, $3::"TmiCategory", $4, $5)
                on conflict (id) do update set
                    "eventId" = excluded."eventId",
                    category = excluded.category,
                    text = excluded.text,
                    "createdAt" = excluded."createdAt"
                "#,
            )
            .bind(&target_id)
            .bind(&event_target_id)
            .bind(&category)
            .bind(&text)
            .bind(row.start_time)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event_tmi",
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

    for row in files {
        let event_target_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let key = row
            .asset_id
            .clone()
            .or_else(|| row.url.clone())
            .unwrap_or_else(|| row.filename.clone());
        let description = match (&row.file_type, &row.url) {
            (Some(file_type), Some(url)) => Some(format!("{file_type} | {url}")),
            (Some(file_type), None) => Some(file_type.clone()),
            (None, Some(url)) => Some(url.clone()),
            (None, None) => None,
        };
        let created_by = match &row.uploaded_by {
            Some(source_user_id) => target::find_mapping(&state.target, "user", source_user_id)
                .await?
                .map(|row| row.target_id),
            None => None,
        };
        let business_key = format!("{event_target_id}:{}", row.filename);
        let target_id = target::find_mapping(&state.target, "ops_plan_file", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "OpsPlanFile" (id, name, description, key, "createdBy", "eventId", "createdAt", "updatedAt")
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    name = excluded.name,
                    description = excluded.description,
                    key = excluded.key,
                    "createdBy" = excluded."createdBy",
                    "eventId" = excluded."eventId",
                    "createdAt" = excluded."createdAt",
                    "updatedAt" = excluded."updatedAt"
                "#,
            )
            .bind(&target_id)
            .bind(&row.filename)
            .bind(&description)
            .bind(&key)
            .bind(&created_by)
            .bind(&event_target_id)
            .bind(row.created_at)
            .bind(row.updated_at)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ops_plan_file",
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
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "events").await?;
    }

    Ok(())
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .with_context(|| format!("missing mapping for {entity_type}/{source_id}"))?
        .target_id)
}
