use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, assume_utc_opt, record_warning},
    mapping::{normalize_event_type, normalize_tmi_category},
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
    archived_at: Option<NaiveDateTime>,
    starts_at: NaiveDateTime,
    ends_at: NaiveDateTime,
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
    user_id: Option<String>,
    requested_position: Option<String>,
    requested_secondary_position: String,
    notes: Option<String>,
    requested_start_time: Option<NaiveDateTime>,
    requested_end_time: Option<NaiveDateTime>,
    final_start_time: Option<NaiveDateTime>,
    final_end_time: Option<NaiveDateTime>,
    final_position: Option<String>,
    final_notes: Option<String>,
    controlling_category: Option<String>,
    is_instructor: bool,
    is_solo: bool,
    is_ots: bool,
    is_tmu: bool,
    is_cic: bool,
    published: bool,
    submitted_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceEventTmi {
    id: String,
    event_id: String,
    category: String,
    text: String,
    created_by: Option<String>,
    created_at: NaiveDateTime,
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
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    let events = sqlx::query_as::<_, SourceEvent>(
        r#"
        select
            id,
            name as title,
            type::text as event_type,
            host,
            description,
            hidden,
            "positionsLocked" as positions_locked,
            "manualPositionsOpen" as manual_positions_open,
            archived as archived_at,
            start as starts_at,
            "end" as ends_at,
            coalesce("featuredFields", '{}'::text[]) as featured_fields,
            coalesce("presetPositions", '{}'::text[]) as preset_positions,
            "enableBufferTimes" as enable_buffer_times,
            "featuredFieldConfigs" as featured_field_configs,
            tmis,
            "opsFreeText" as ops_free_text,
            "opsPlanPublished" as ops_plan_published,
            "opsPlannerId" as ops_planner_id,
            "bannerKey" as banner_asset_id
        from public."Event"
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let positions = sqlx::query_as::<_, SourceEventPosition>(
        r#"
        select
            id,
            "eventId" as event_id,
            "userId" as user_id,
            "requestedPosition" as requested_position,
            "requestedSecondaryPosition" as requested_secondary_position,
            notes,
            "requestedStartTime" as requested_start_time,
            "requestedEndTime" as requested_end_time,
            "finalStartTime" as final_start_time,
            "finalEndTime" as final_end_time,
            "finalPosition" as final_position,
            "finalNotes" as final_notes,
            "controllingCategory"::text as controlling_category,
            "isInstructor" as is_instructor,
            "isSolo" as is_solo,
            "isOts" as is_ots,
            "isTmu" as is_tmu,
            "isCic" as is_cic,
            published,
            "submittedAt" as submitted_at
        from public."EventPosition"
        order by "submittedAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let tmis = sqlx::query_as::<_, SourceEventTmi>(
        r#"
        select
            id,
            "eventId" as event_id,
            category::text as category,
            text,
            "createdBy" as created_by,
            "createdAt" as created_at
        from public."EventTmi"
        order by "createdAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let files = sqlx::query_as::<_, SourceOpsPlanFile>(
        r#"
        select
            id,
            "eventId" as event_id,
            nullif(key, '') as asset_id,
            name as filename,
            null::text as url,
            null::text as file_type,
            "createdBy" as uploaded_by,
            "createdAt" as created_at,
            "updatedAt" as updated_at
        from public."OpsPlanFile"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    let positions_by_event = positions.iter().cloned().fold(
        HashMap::<String, Vec<SourceEventPosition>>::new(),
        |mut acc, row| {
            acc.entry(row.event_id.clone()).or_default().push(row);
            acc
        },
    );
    let fallback_creator_id = sqlx::query_scalar::<_, String>(
        r#"select id from identity.users order by joined_at asc limit 1"#,
    )
    .fetch_optional(&state.target)
    .await?;

    for row in &events {
        state.report.domain_mut(DOMAIN).planned += 1;
        let target_id = mapped_or_same(&state.target, "event", &row.id).await?;
        let existed = exists(&state.target, "events.events", &target_id).await?;
        let source_business_key = format!("{}:{}:{}", row.title, row.starts_at, row.ends_at);
        let ops_planner_id = if let Some(id) = row.ops_planner_id.as_deref() {
            Some(mapped_id(&state.target, "user", id).await?)
        } else {
            None
        };
        let status = derive_event_status(row.archived_at, row.hidden, row.ops_plan_published);
        let published = row.ops_plan_published || !row.hidden;

        if !state.config.dry_run {
            let created_by = derive_created_by(
                &ops_planner_id,
                positions_by_event.get(&row.id),
                fallback_creator_id.as_deref(),
            )
            .context("unable to derive event created_by user")?;
            sqlx::query(
                r#"
                insert into events.events (
                    id, title, type, host, description, status, published, banner_asset_id,
                    hidden, positions_locked, manual_positions_open, archived_at, starts_at, ends_at,
                    featured_fields, preset_positions, enable_buffer_times, featured_field_configs,
                    tmis, ops_free_text, ops_plan_published, ops_planner_id, created_by
                )
                values (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                    $17, $18, $19, $20, $21, $22, $23
                )
                on conflict (id) do update set
                    title = excluded.title,
                    type = excluded.type,
                    host = excluded.host,
                    description = excluded.description,
                    status = excluded.status,
                    published = excluded.published,
                    banner_asset_id = excluded.banner_asset_id,
                    hidden = excluded.hidden,
                    positions_locked = excluded.positions_locked,
                    manual_positions_open = excluded.manual_positions_open,
                    archived_at = excluded.archived_at,
                    starts_at = excluded.starts_at,
                    ends_at = excluded.ends_at,
                    featured_fields = excluded.featured_fields,
                    preset_positions = excluded.preset_positions,
                    enable_buffer_times = excluded.enable_buffer_times,
                    featured_field_configs = excluded.featured_field_configs,
                    tmis = excluded.tmis,
                    ops_free_text = excluded.ops_free_text,
                    ops_plan_published = excluded.ops_plan_published,
                    ops_planner_id = excluded.ops_planner_id,
                    created_by = excluded.created_by
                "#,
            )
            .bind(&target_id)
            .bind(&row.title)
            .bind(normalize_event_type(&row.event_type)?)
            .bind(&row.host)
            .bind(&row.description)
            .bind(status)
            .bind(published)
            .bind(&row.banner_asset_id)
            .bind(row.hidden)
            .bind(row.positions_locked)
            .bind(row.manual_positions_open)
            .bind(assume_utc_opt(row.archived_at))
            .bind(assume_utc(row.starts_at))
            .bind(assume_utc(row.ends_at))
            .bind(&row.featured_fields)
            .bind(&row.preset_positions)
            .bind(row.enable_buffer_times)
            .bind(&row.featured_field_configs)
            .bind(&row.tmis)
            .bind(&row.ops_free_text)
            .bind(row.ops_plan_published)
            .bind(&ops_planner_id)
            .bind(&created_by)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event",
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

    let mut seen_position_keys = HashSet::new();
    for row in positions {
        let event_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let user_id = if let Some(id) = row.user_id.as_deref() {
            Some(mapped_id(&state.target, "user", id).await?)
        } else {
            None
        };
        let callsign = derive_callsign(&row);
        if !seen_position_keys.insert((event_id.clone(), callsign.clone())) {
            record_warning(
                state,
                DOMAIN,
                "event_position",
                &row.id,
                format!(
                    "skipping duplicate event position for event `{event_id}` and callsign `{callsign}`"
                ),
            )
            .await?;
            continue;
        }
        let target_id = mapped_or_same(&state.target, "event_position", &row.id).await?;
        let status = derive_position_status(&row, user_id.is_some());

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into events.event_positions (
                    id, event_id, callsign, user_id, requested_position, requested_secondary_position,
                    notes, requested_start_time, requested_end_time, final_start_time, final_end_time,
                    final_position, final_notes, controlling_category, is_instructor, is_solo, is_ots,
                    is_tmu, is_cic, published, status, submitted_at
                )
                values (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                    $18, $19, $20, $21, $22
                )
                on conflict (id) do update set
                    event_id = excluded.event_id,
                    callsign = excluded.callsign,
                    user_id = excluded.user_id,
                    requested_position = excluded.requested_position,
                    requested_secondary_position = excluded.requested_secondary_position,
                    notes = excluded.notes,
                    requested_start_time = excluded.requested_start_time,
                    requested_end_time = excluded.requested_end_time,
                    final_start_time = excluded.final_start_time,
                    final_end_time = excluded.final_end_time,
                    final_position = excluded.final_position,
                    final_notes = excluded.final_notes,
                    controlling_category = excluded.controlling_category,
                    is_instructor = excluded.is_instructor,
                    is_solo = excluded.is_solo,
                    is_ots = excluded.is_ots,
                    is_tmu = excluded.is_tmu,
                    is_cic = excluded.is_cic,
                    published = excluded.published,
                    status = excluded.status,
                    submitted_at = excluded.submitted_at
                "#,
            )
            .bind(&target_id)
            .bind(&event_id)
            .bind(&callsign)
            .bind(&user_id)
            .bind(&row.requested_position)
            .bind(&row.requested_secondary_position)
            .bind(&row.notes)
            .bind(assume_utc_opt(row.requested_start_time))
            .bind(assume_utc_opt(row.requested_end_time))
            .bind(assume_utc_opt(row.final_start_time))
            .bind(assume_utc_opt(row.final_end_time))
            .bind(&row.final_position)
            .bind(&row.final_notes)
            .bind(&row.controlling_category)
            .bind(row.is_instructor)
            .bind(row.is_solo)
            .bind(row.is_ots)
            .bind(row.is_tmu)
            .bind(row.is_cic)
            .bind(row.published)
            .bind(status)
            .bind(assume_utc(row.submitted_at))
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event_position",
                &row.id,
                &format!("{event_id}:{callsign}"),
                &target_id,
                &format!("{event_id}:{callsign}"),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in tmis {
        let event_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let target_id = mapped_or_same(&state.target, "event_tmi", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into events.event_tmis (id, event_id, tmi_type, start_time, notes)
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    event_id = excluded.event_id,
                    tmi_type = excluded.tmi_type,
                    start_time = excluded.start_time,
                    notes = excluded.notes
                "#,
            )
            .bind(&target_id)
            .bind(&event_id)
            .bind(normalize_tmi_category(&row.category)?)
            .bind(assume_utc(row.created_at))
            .bind(&row.text)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "event_tmi",
                &row.id,
                &format!("{event_id}:{}", row.created_at),
                &target_id,
                &format!("{event_id}:{}", row.created_at),
                "updated",
                &row,
            )
            .await?;
        }
    }

    for row in files {
        let event_id = mapped_id(&state.target, "event", &row.event_id).await?;
        let uploaded_by = if let Some(id) = row.uploaded_by.as_deref() {
            Some(mapped_id(&state.target, "user", id).await?)
        } else {
            None
        };
        let target_id = mapped_or_same(&state.target, "ops_plan_file", &row.id).await?;
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into events.ops_plan_files (
                    id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                on conflict (id) do update set
                    event_id = excluded.event_id,
                    asset_id = excluded.asset_id,
                    filename = excluded.filename,
                    url = excluded.url,
                    file_type = excluded.file_type,
                    uploaded_by = excluded.uploaded_by,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(&target_id)
            .bind(&event_id)
            .bind(&row.asset_id)
            .bind(&row.filename)
            .bind(&row.url)
            .bind(&row.file_type)
            .bind(&uploaded_by)
            .bind(assume_utc(row.created_at))
            .bind(assume_utc(row.updated_at))
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "ops_plan_file",
                &row.id,
                &format!("{event_id}:{}", row.filename),
                &target_id,
                &format!("{event_id}:{}", row.filename),
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

fn derive_event_status(
    archived_at: Option<NaiveDateTime>,
    hidden: bool,
    ops_plan_published: bool,
) -> &'static str {
    if archived_at.is_some() {
        "ARCHIVED"
    } else if ops_plan_published && !hidden {
        "PUBLISHED"
    } else {
        "SCHEDULED"
    }
}

fn derive_position_status(row: &SourceEventPosition, has_user: bool) -> &'static str {
    if row.published {
        "PUBLISHED"
    } else if has_user || row.final_position.is_some() {
        "ASSIGNED"
    } else if row.requested_position.is_some()
        || !row.requested_secondary_position.trim().is_empty()
    {
        "REQUESTED"
    } else {
        "OPEN"
    }
}

fn derive_callsign(row: &SourceEventPosition) -> String {
    row.final_position
        .clone()
        .or_else(|| row.requested_position.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if row.requested_secondary_position.trim().is_empty() {
                format!("legacy-slot-{}", &row.id[..row.id.len().min(8)])
            } else {
                row.requested_secondary_position.clone()
            }
        })
}

fn derive_created_by(
    ops_planner_id: &Option<String>,
    positions: Option<&Vec<SourceEventPosition>>,
    fallback_user_id: Option<&str>,
) -> Option<String> {
    if let Some(user_id) = ops_planner_id.clone() {
        return Some(user_id);
    }
    if let Some(user_id) =
        positions.and_then(|rows| rows.iter().find_map(|row| row.user_id.clone()))
    {
        return Some(user_id);
    }
    fallback_user_id.map(ToOwned::to_owned)
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
