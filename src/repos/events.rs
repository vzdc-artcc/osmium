use sqlx::PgPool;

use crate::{
    errors::ApiError,
    models::{
        Event, EventOpsPlanItem, EventPosition, EventPositionPreset, EventTmiItem, OpsPlanFile,
        UserEventPositionItem,
    },
};

#[derive(Debug, sqlx::FromRow)]
struct EventRow {
    id: String,
    title: String,
    event_type: Option<String>,
    host: Option<String>,
    description: Option<String>,
    status: String,
    published: bool,
    banner_asset_id: Option<String>,
    hidden: bool,
    positions_locked: bool,
    manual_positions_open: bool,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    created_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

const EVENT_COLUMNS: &str = "id, title, type AS event_type, host, description, status, published, banner_asset_id, hidden, positions_locked, manual_positions_open, archived_at, starts_at, ends_at, created_by, created_at, updated_at";

impl From<EventRow> for Event {
    fn from(row: EventRow) -> Self {
        Event {
            id: row.id,
            title: row.title,
            event_type: row.event_type,
            host: row.host,
            description: row.description,
            status: row.status,
            published: row.published,
            banner_asset_id: row.banner_asset_id,
            hidden: row.hidden,
            positions_locked: row.positions_locked,
            manual_positions_open: row.manual_positions_open,
            archived_at: row.archived_at,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            created_by: row.created_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EventPositionRow {
    id: String,
    event_id: String,
    callsign: String,
    user_id: Option<String>,
    user_cid: Option<i64>,
    user_name: Option<String>,
    user_rating: Option<String>,
    user_controller_status: Option<String>,
    user_discord_id: Option<String>,
    requested_slot: Option<i32>,
    assigned_slot: Option<i32>,
    requested_position: Option<String>,
    requested_secondary_position: String,
    notes: Option<String>,
    requested_start_time: Option<chrono::DateTime<chrono::Utc>>,
    requested_end_time: Option<chrono::DateTime<chrono::Utc>>,
    final_start_time: Option<chrono::DateTime<chrono::Utc>>,
    final_end_time: Option<chrono::DateTime<chrono::Utc>>,
    final_position: Option<String>,
    final_notes: Option<String>,
    controlling_category: Option<String>,
    is_instructor: bool,
    is_solo: bool,
    is_ots: bool,
    is_tmu: bool,
    is_cic: bool,
    published: bool,
    status: String,
    submitted_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

const EVENT_POSITION_COLUMNS: &str = "ep.id, ep.event_id, ep.callsign, ep.user_id, u.cid as user_cid, u.display_name as user_name, m.rating as user_rating, m.controller_status as user_controller_status, dl.external_id as user_discord_id, ep.requested_slot, ep.assigned_slot, ep.requested_position, ep.requested_secondary_position, ep.notes, ep.requested_start_time, ep.requested_end_time, ep.final_start_time, ep.final_end_time, ep.final_position, ep.final_notes, ep.controlling_category, ep.is_instructor, ep.is_solo, ep.is_ots, ep.is_tmu, ep.is_cic, ep.published, ep.status, ep.submitted_at, ep.created_at, ep.updated_at";
const EVENT_POSITION_FROM: &str = "events.event_positions ep \
    left join identity.users u on u.id = ep.user_id \
    left join org.memberships m on m.user_id = ep.user_id \
    left join integration.external_sync_mappings dl on dl.system_code = 'discord' and dl.entity_type = 'user_identity' and dl.local_id = ep.user_id";

impl From<EventPositionRow> for EventPosition {
    fn from(row: EventPositionRow) -> Self {
        EventPosition {
            id: row.id,
            event_id: row.event_id,
            callsign: row.callsign,
            user_id: row.user_id,
            user_cid: row.user_cid,
            user_name: row.user_name,
            user_rating: row.user_rating,
            user_controller_status: row.user_controller_status,
            user_discord_id: row.user_discord_id,
            requested_slot: row.requested_slot,
            assigned_slot: row.assigned_slot,
            requested_position: row.requested_position,
            requested_secondary_position: row.requested_secondary_position,
            notes: row.notes,
            requested_start_time: row.requested_start_time,
            requested_end_time: row.requested_end_time,
            final_start_time: row.final_start_time,
            final_end_time: row.final_end_time,
            final_position: row.final_position,
            final_notes: row.final_notes,
            controlling_category: row.controlling_category,
            is_instructor: row.is_instructor,
            is_solo: row.is_solo,
            is_ots: row.is_ots,
            is_tmu: row.is_tmu,
            is_cic: row.is_cic,
            published: row.published,
            status: row.status,
            submitted_at: row.submitted_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EventTmiItemRow {
    id: String,
    event_id: String,
    tmi_type: String,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    notes: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<EventTmiItemRow> for EventTmiItem {
    fn from(row: EventTmiItemRow) -> Self {
        EventTmiItem {
            id: row.id,
            event_id: row.event_id,
            tmi_type: row.tmi_type,
            start_time: row.start_time,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn count_events(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from events.events")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_events(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<Event>, ApiError> {
    sqlx::query_as::<_, EventRow>(&format!(
        "SELECT {EVENT_COLUMNS} FROM events.events ORDER BY starts_at DESC, id ASC LIMIT $1 OFFSET $2"
    ))
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_event(pool: &PgPool, event_id: &str) -> Result<Option<Event>, ApiError> {
    sqlx::query_as::<_, EventRow>(&format!(
        "SELECT {EVENT_COLUMNS} FROM events.events WHERE id = $1"
    ))
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_event(
    pool: &PgPool,
    id: &str,
    title: &str,
    event_type: Option<&str>,
    host: Option<&str>,
    description: Option<&str>,
    banner_asset_id: Option<&str>,
    status: &str,
    published: bool,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    created_by: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Event, ApiError> {
    sqlx::query_as::<_, EventRow>(&format!(
        "INSERT INTO events.events (id, title, type, host, description, banner_asset_id, status, published, starts_at, ends_at, created_by, created_at, updated_at)
         VALUES ($1, $2, COALESCE($3, 'STANDARD'), $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING {EVENT_COLUMNS}"
    ))
    .bind(id)
    .bind(title)
    .bind(event_type)
    .bind(host)
    .bind(description)
    .bind(banner_asset_id)
    .bind(status)
    .bind(published)
    .bind(starts_at)
    .bind(ends_at)
    .bind(created_by)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_event_row(
    pool: &PgPool,
    event_id: &str,
    title: Option<String>,
    event_type: Option<String>,
    host: Option<String>,
    description: Option<String>,
    status: Option<String>,
    published: Option<bool>,
    banner_asset_id_set: bool,
    banner_asset_id: Option<String>,
    hidden: Option<bool>,
    manual_positions_open: Option<bool>,
    archived: Option<bool>,
    starts_at: Option<chrono::DateTime<chrono::Utc>>,
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<Event>, ApiError> {
    sqlx::query_as::<_, EventRow>(&format!(
        "UPDATE events.events SET
            title = COALESCE($1, title),
            type = COALESCE($2, type),
            host = COALESCE($3, host),
            description = COALESCE($4, description),
            status = COALESCE($5, status),
            published = COALESCE($6, published),
            banner_asset_id = CASE WHEN $7::bool THEN $8 ELSE banner_asset_id END,
            hidden = COALESCE($9, hidden),
            manual_positions_open = COALESCE($10, manual_positions_open),
            archived_at = CASE
                WHEN $11::bool IS NULL THEN archived_at
                WHEN $11::bool THEN COALESCE(archived_at, $14)
                ELSE NULL
            END,
            starts_at = COALESCE($12, starts_at),
            ends_at = COALESCE($13, ends_at),
            updated_at = $14
         WHERE id = $15
         RETURNING {EVENT_COLUMNS}"
    ))
    .bind(title)
    .bind(event_type)
    .bind(host)
    .bind(description)
    .bind(status)
    .bind(published)
    .bind(banner_asset_id_set)
    .bind(banner_asset_id)
    .bind(hidden)
    .bind(manual_positions_open)
    .bind(archived)
    .bind(starts_at)
    .bind(ends_at)
    .bind(now)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_event_row(pool: &PgPool, event_id: &str) -> Result<u64, ApiError> {
    let result = sqlx::query("DELETE FROM events.events WHERE id = $1")
        .bind(event_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}

pub async fn count_event_positions(pool: &PgPool, event_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM events.event_positions WHERE event_id = $1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_event_positions(
    pool: &PgPool,
    event_id: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<EventPosition>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(&format!(
        "SELECT {EVENT_POSITION_COLUMNS} FROM {EVENT_POSITION_FROM} WHERE ep.event_id = $1 ORDER BY ep.assigned_slot ASC NULLS LAST, ep.id ASC LIMIT $2 OFFSET $3"
    ))
    .bind(event_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn list_event_positions_all(
    pool: &PgPool,
    event_id: &str,
) -> Result<Vec<EventPosition>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(&format!(
        "SELECT {EVENT_POSITION_COLUMNS} FROM {EVENT_POSITION_FROM} WHERE ep.event_id = $1 ORDER BY ep.assigned_slot ASC NULLS LAST"
    ))
    .bind(event_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn count_user_published_event_positions(
    pool: &PgPool,
    cid: i64,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from events.event_positions ep
        join identity.users u on u.id = ep.user_id
        where u.cid = $1 and ep.published = true
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_published_event_positions(
    pool: &PgPool,
    cid: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<UserEventPositionItem>, ApiError> {
    sqlx::query_as::<_, UserEventPositionItem>(
        r#"
        select
            ep.id,
            ep.event_id,
            e.title as event_title,
            e.starts_at as event_starts_at,
            e.type as event_type,
            ep.final_position,
            ep.final_start_time,
            ep.final_end_time
        from events.event_positions ep
        join events.events e on e.id = ep.event_id
        join identity.users u on u.id = ep.user_id
        where u.cid = $1 and ep.published = true
        order by e.starts_at desc, ep.id asc
        limit $2 offset $3
        "#,
    )
    .bind(cid)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_event_position(
    pool: &PgPool,
    id: &str,
    event_id: &str,
    user_id: &str,
    requested_position: &str,
    requested_secondary_position: &str,
    notes: Option<&str>,
    requested_start_time: chrono::DateTime<chrono::Utc>,
    requested_end_time: chrono::DateTime<chrono::Utc>,
    final_position: Option<&str>,
    final_start_time: Option<chrono::DateTime<chrono::Utc>>,
    final_end_time: Option<chrono::DateTime<chrono::Utc>>,
    final_notes: Option<&str>,
    controlling_category: Option<&str>,
    is_instructor: bool,
    is_solo: bool,
    is_ots: bool,
    is_tmu: bool,
    is_cic: bool,
    status: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<EventPosition, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(&format!(
        "WITH inserted AS (
            INSERT INTO events.event_positions (
                id, event_id, callsign, user_id, requested_position, requested_secondary_position,
                notes, requested_start_time, requested_end_time, final_position, final_start_time,
                final_end_time, final_notes, controlling_category, is_instructor, is_solo, is_ots,
                is_tmu, is_cic, status, published, submitted_at, created_at, updated_at
             )
             VALUES ($1, $2, $3, $4, $3, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $21, $21)
             RETURNING *
         )
         SELECT {EVENT_POSITION_COLUMNS} FROM inserted ep LEFT JOIN identity.users u ON u.id = ep.user_id LEFT JOIN org.memberships m ON m.user_id = ep.user_id LEFT JOIN integration.external_sync_mappings dl ON dl.system_code = 'discord' AND dl.entity_type = 'user_identity' AND dl.local_id = ep.user_id"
    ))
    .bind(id)
    .bind(event_id)
    .bind(requested_position)
    .bind(user_id)
    .bind(requested_secondary_position)
    .bind(notes)
    .bind(requested_start_time)
    .bind(requested_end_time)
    .bind(final_position)
    .bind(final_start_time)
    .bind(final_end_time)
    .bind(final_notes)
    .bind(controlling_category)
    .bind(is_instructor)
    .bind(is_solo)
    .bind(is_ots)
    .bind(is_tmu)
    .bind(is_cic)
    .bind(status)
    .bind(false)
    .bind(now)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|error| match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => ApiError::BadRequest,
        _ => ApiError::Internal,
    })
}

pub async fn fetch_event_position(
    pool: &PgPool,
    position_id: &str,
    event_id: &str,
) -> Result<Option<EventPosition>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(&format!(
        "SELECT {EVENT_POSITION_COLUMNS} FROM {EVENT_POSITION_FROM} WHERE ep.id = $1 AND ep.event_id = $2"
    ))
    .bind(position_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_event_position_row(
    pool: &PgPool,
    position_id: &str,
    event_id: &str,
    user_id_set: bool,
    user_id: Option<String>,
    assigned_slot: Option<i32>,
    final_position_set: bool,
    final_position: Option<String>,
    final_start_time_set: bool,
    final_start_time: Option<chrono::DateTime<chrono::Utc>>,
    final_end_time_set: bool,
    final_end_time: Option<chrono::DateTime<chrono::Utc>>,
    final_notes_set: bool,
    final_notes: Option<String>,
    controlling_category_set: bool,
    controlling_category: Option<String>,
    is_instructor: Option<bool>,
    is_solo: Option<bool>,
    is_ots: Option<bool>,
    is_tmu: Option<bool>,
    is_cic: Option<bool>,
    published: Option<bool>,
    status: Option<String>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<EventPosition>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(&format!(
        r#"
        WITH updated AS (
            UPDATE events.event_positions SET
                user_id = CASE WHEN $3::bool THEN $4 ELSE user_id END,
                assigned_slot = COALESCE($5, assigned_slot),
                final_position = CASE WHEN $6::bool THEN $7 ELSE final_position END,
                final_start_time = CASE WHEN $8::bool THEN $9 ELSE final_start_time END,
                final_end_time = CASE WHEN $10::bool THEN $11 ELSE final_end_time END,
                final_notes = CASE WHEN $12::bool THEN $13 ELSE final_notes END,
                controlling_category = CASE WHEN $14::bool THEN $15 ELSE controlling_category END,
                is_instructor = COALESCE($16, is_instructor),
                is_solo = COALESCE($17, is_solo),
                is_ots = COALESCE($18, is_ots),
                is_tmu = COALESCE($19, is_tmu),
                is_cic = COALESCE($20, is_cic),
                published = COALESCE($21, published),
                status = COALESCE($22, status),
                updated_at = $23
             WHERE id = $1 AND event_id = $2
             RETURNING *
        )
        SELECT {EVENT_POSITION_COLUMNS} FROM updated ep LEFT JOIN identity.users u ON u.id = ep.user_id LEFT JOIN org.memberships m ON m.user_id = ep.user_id LEFT JOIN integration.external_sync_mappings dl ON dl.system_code = 'discord' AND dl.entity_type = 'user_identity' AND dl.local_id = ep.user_id
        "#
    ))
    .bind(position_id)
    .bind(event_id)
    .bind(user_id_set)
    .bind(user_id)
    .bind(assigned_slot)
    .bind(final_position_set)
    .bind(final_position)
    .bind(final_start_time_set)
    .bind(final_start_time)
    .bind(final_end_time_set)
    .bind(final_end_time)
    .bind(final_notes_set)
    .bind(final_notes)
    .bind(controlling_category_set)
    .bind(controlling_category)
    .bind(is_instructor)
    .bind(is_solo)
    .bind(is_ots)
    .bind(is_tmu)
    .bind(is_cic)
    .bind(published)
    .bind(status)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_event_position_row(
    pool: &PgPool,
    position_id: &str,
    event_id: &str,
) -> Result<u64, ApiError> {
    let result = sqlx::query("DELETE FROM events.event_positions WHERE id = $1 AND event_id = $2")
        .bind(position_id)
        .bind(event_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}

pub async fn set_positions_published(pool: &PgPool, event_id: &str) -> Result<(), ApiError> {
    sqlx::query("UPDATE events.event_positions SET published = true WHERE event_id = $1")
        .bind(event_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_event_ops_plan(
    pool: &PgPool,
    event_id: &str,
) -> Result<Option<EventOpsPlanItem>, ApiError> {
    sqlx::query_as::<_, EventOpsPlanItem>(
        r#"
        select e.id, e.title, e.positions_locked, e.manual_positions_open, e.featured_fields, e.preset_positions,
               e.featured_field_configs, e.tmis, e.ops_free_text, e.ops_plan_published, e.ops_planner_id,
               u.cid as ops_planner_cid, u.display_name as ops_planner_name,
               e.enable_buffer_times, e.updated_at
        from events.events e
        left join identity.users u on u.id = e.ops_planner_id
        where e.id = $1
        "#,
    ).bind(event_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_event_ops_plan_row(
    pool: &PgPool,
    event_id: &str,
    featured_fields: Option<Vec<String>>,
    preset_positions: Option<Vec<String>>,
    featured_field_configs: Option<serde_json::Value>,
    tmis_set: bool,
    tmis: Option<String>,
    ops_free_text_set: bool,
    ops_free_text: Option<String>,
    ops_plan_published: Option<bool>,
    ops_planner_id_set: bool,
    ops_planner_id: Option<String>,
    enable_buffer_times: Option<bool>,
) -> Result<Option<EventOpsPlanItem>, ApiError> {
    sqlx::query_as::<_, EventOpsPlanItem>(
        r#"
        with updated as (
            update events.events
            set featured_fields = coalesce($2, featured_fields),
                preset_positions = coalesce($3, preset_positions),
                featured_field_configs = coalesce($4, featured_field_configs),
                tmis = case when $5::bool then $6 else tmis end,
                ops_free_text = case when $7::bool then $8 else ops_free_text end,
                ops_plan_published = coalesce($9, ops_plan_published),
                ops_planner_id = case when $10::bool then $11 else ops_planner_id end,
                enable_buffer_times = coalesce($12, enable_buffer_times),
                updated_at = now()
            where id = $1
            returning *
        )
        select e.id, e.title, e.positions_locked, e.manual_positions_open, e.featured_fields, e.preset_positions,
               e.featured_field_configs, e.tmis, e.ops_free_text, e.ops_plan_published, e.ops_planner_id,
               u.cid as ops_planner_cid, u.display_name as ops_planner_name,
               e.enable_buffer_times, e.updated_at
        from updated e
        left join identity.users u on u.id = e.ops_planner_id
        "#,
    )
    .bind(event_id)
    .bind(featured_fields)
    .bind(preset_positions)
    .bind(featured_field_configs)
    .bind(tmis_set)
    .bind(tmis)
    .bind(ops_free_text_set)
    .bind(ops_free_text)
    .bind(ops_plan_published)
    .bind(ops_planner_id_set)
    .bind(ops_planner_id)
    .bind(enable_buffer_times)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_event_tmis(pool: &PgPool, event_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from events.event_tmis where event_id = $1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_event_tmis(
    pool: &PgPool,
    event_id: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<EventTmiItem>, ApiError> {
    sqlx::query_as::<_, EventTmiItemRow>(
        "select id, event_id, tmi_type, start_time, notes, created_at, updated_at from events.event_tmis where event_id = $1 order by created_at asc, id asc limit $2 offset $3",
    )
    .bind(event_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_event_tmi(
    pool: &PgPool,
    id: &str,
    event_id: &str,
    tmi_type: &str,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    notes: Option<&str>,
) -> Result<EventTmiItem, ApiError> {
    sqlx::query_as::<_, EventTmiItemRow>(
        "insert into events.event_tmis (id, event_id, tmi_type, start_time, notes, created_at, updated_at) values ($1, $2, $3, $4, $5, now(), now()) returning id, event_id, tmi_type, start_time, notes, created_at, updated_at",
    ).bind(id).bind(event_id).bind(tmi_type).bind(start_time).bind(notes).fetch_one(pool).await.map(Into::into).map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_event_tmi(
    pool: &PgPool,
    event_id: &str,
    tmi_id: &str,
) -> Result<Option<EventTmiItem>, ApiError> {
    sqlx::query_as::<_, EventTmiItemRow>(
        "select id, event_id, tmi_type, start_time, notes, created_at, updated_at from events.event_tmis where event_id = $1 and id = $2",
    ).bind(event_id).bind(tmi_id).fetch_optional(pool).await.map(|row| row.map(Into::into)).map_err(|_| ApiError::Internal)
}

pub async fn update_event_tmi_row(
    pool: &PgPool,
    event_id: &str,
    tmi_id: &str,
    tmi_type: Option<&str>,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    notes_set: bool,
    notes: Option<String>,
) -> Result<Option<EventTmiItem>, ApiError> {
    sqlx::query_as::<_, EventTmiItemRow>(
        r#"
        update events.event_tmis
        set tmi_type = coalesce($3, tmi_type),
            start_time = coalesce($4, start_time),
            notes = case when $5::bool then $6 else notes end,
            updated_at = now()
        where event_id = $1 and id = $2
        returning id, event_id, tmi_type, start_time, notes, created_at, updated_at
        "#,
    )
    .bind(event_id)
    .bind(tmi_id)
    .bind(tmi_type)
    .bind(start_time)
    .bind(notes_set)
    .bind(notes)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_event_tmi_row(
    pool: &PgPool,
    event_id: &str,
    tmi_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from events.event_tmis where event_id = $1 and id = $2")
        .bind(event_id)
        .bind(tmi_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_preset_positions(
    pool: &PgPool,
    event_id: &str,
) -> Result<Option<Vec<String>>, ApiError> {
    sqlx::query_scalar::<_, Vec<String>>("select preset_positions from events.events where id = $1")
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn update_preset_positions_row(
    pool: &PgPool,
    event_id: &str,
    preset_positions: &[String],
) -> Result<(), ApiError> {
    sqlx::query("update events.events set preset_positions = $2, updated_at = now() where id = $1")
        .bind(event_id)
        .bind(preset_positions)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn set_positions_locked(
    pool: &PgPool,
    event_id: &str,
    locked: bool,
) -> Result<(), ApiError> {
    sqlx::query("update events.events set positions_locked = $2, updated_at = now() where id = $1")
        .bind(event_id)
        .bind(locked)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

// --- Named event-position-preset bundles (events.event_position_presets) ---

pub async fn count_event_position_presets(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from events.event_position_presets")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_event_position_presets(
    pool: &PgPool,
    page_size: i64,
    offset: i64,
) -> Result<Vec<EventPositionPreset>, ApiError> {
    sqlx::query_as::<_, EventPositionPreset>(
        "select id, name, positions, created_at, updated_at from events.event_position_presets order by name asc limit $1 offset $2",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_event_position_preset(
    pool: &PgPool,
    preset_id: &str,
) -> Result<Option<EventPositionPreset>, ApiError> {
    sqlx::query_as::<_, EventPositionPreset>(
        "select id, name, positions, created_at, updated_at from events.event_position_presets where id = $1",
    )
    .bind(preset_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_event_position_preset(
    pool: &PgPool,
    id: &str,
    name: &str,
    positions: &[String],
) -> Result<EventPositionPreset, ApiError> {
    sqlx::query_as::<_, EventPositionPreset>(
        "insert into events.event_position_presets (id, name, positions, created_at, updated_at) values ($1, $2, $3, now(), now()) returning id, name, positions, created_at, updated_at",
    )
    .bind(id)
    .bind(name)
    .bind(positions)
    .fetch_one(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => ApiError::BadRequest,
        _ => ApiError::Internal,
    })
}

pub async fn update_event_position_preset_row(
    pool: &PgPool,
    preset_id: &str,
    name: Option<&str>,
    positions: Option<&[String]>,
) -> Result<Option<EventPositionPreset>, ApiError> {
    sqlx::query_as::<_, EventPositionPreset>(
        r#"
        update events.event_position_presets
        set name = coalesce($2, name),
            positions = coalesce($3, positions),
            updated_at = now()
        where id = $1
        returning id, name, positions, created_at, updated_at
        "#,
    )
    .bind(preset_id)
    .bind(name)
    .bind(positions)
    .fetch_optional(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => ApiError::BadRequest,
        _ => ApiError::Internal,
    })
}

pub async fn delete_event_position_preset_row(
    pool: &PgPool,
    preset_id: &str,
) -> Result<u64, ApiError> {
    let result = sqlx::query("delete from events.event_position_presets where id = $1")
        .bind(preset_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}

// --- Ops plan file attachments (events.ops_plan_files) ---

pub async fn list_ops_plan_files(
    pool: &PgPool,
    event_id: &str,
) -> Result<Vec<OpsPlanFile>, ApiError> {
    sqlx::query_as::<_, OpsPlanFile>(
        "select id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at from events.ops_plan_files where event_id = $1 order by created_at asc",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_ops_plan_file(
    pool: &PgPool,
    event_id: &str,
    file_id: &str,
) -> Result<Option<OpsPlanFile>, ApiError> {
    sqlx::query_as::<_, OpsPlanFile>(
        "select id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at from events.ops_plan_files where event_id = $1 and id = $2",
    )
    .bind(event_id)
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_ops_plan_file(
    pool: &PgPool,
    id: &str,
    event_id: &str,
    asset_id: Option<&str>,
    filename: &str,
    url: Option<&str>,
    file_type: Option<&str>,
    uploaded_by: &str,
) -> Result<OpsPlanFile, ApiError> {
    sqlx::query_as::<_, OpsPlanFile>(
        "insert into events.ops_plan_files (id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at)
         values ($1, $2, $3, $4, $5, $6, $7, now(), now())
         returning id, event_id, asset_id, filename, url, file_type, uploaded_by, created_at, updated_at",
    )
    .bind(id)
    .bind(event_id)
    .bind(asset_id)
    .bind(filename)
    .bind(url)
    .bind(file_type)
    .bind(uploaded_by)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_ops_plan_file_row(
    pool: &PgPool,
    event_id: &str,
    file_id: &str,
) -> Result<u64, ApiError> {
    let result = sqlx::query("delete from events.ops_plan_files where event_id = $1 and id = $2")
        .bind(event_id)
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}
