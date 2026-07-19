use sqlx::PgPool;

use crate::{
    errors::ApiError,
    models::{Event, EventOpsPlanItem, EventPosition, EventTmiItem, UserEventPositionItem},
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
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    created_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

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
    requested_slot: Option<i32>,
    assigned_slot: Option<i32>,
    published: bool,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<EventPositionRow> for EventPosition {
    fn from(row: EventPositionRow) -> Self {
        EventPosition {
            id: row.id,
            event_id: row.event_id,
            callsign: row.callsign,
            user_id: row.user_id,
            requested_slot: row.requested_slot,
            assigned_slot: row.assigned_slot,
            published: row.published,
            status: row.status,
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
    start_time: chrono::DateTime<chrono::Utc>,
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
    sqlx::query_as::<_, EventRow>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events ORDER BY starts_at DESC, id ASC LIMIT $1 OFFSET $2"
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_event(pool: &PgPool, event_id: &str) -> Result<Option<Event>, ApiError> {
    sqlx::query_as::<_, EventRow>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events WHERE id = $1"
    )
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
    status: &str,
    published: bool,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    created_by: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Event, ApiError> {
    sqlx::query_as::<_, EventRow>(
        "INSERT INTO events.events (id, title, type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at)
         VALUES ($1, $2, COALESCE($3, 'STANDARD'), $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at"
    )
    .bind(id)
    .bind(title)
    .bind(event_type)
    .bind(host)
    .bind(description)
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
    starts_at: Option<chrono::DateTime<chrono::Utc>>,
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<Event>, ApiError> {
    sqlx::query_as::<_, EventRow>(
        "UPDATE events.events SET
            title = COALESCE($1, title),
            type = COALESCE($2, type),
            host = COALESCE($3, host),
            description = COALESCE($4, description),
            status = COALESCE($5, status),
            published = COALESCE($6, published),
            starts_at = COALESCE($7, starts_at),
            ends_at = COALESCE($8, ends_at),
            updated_at = $9
         WHERE id = $10
         RETURNING id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at"
    )
    .bind(title)
    .bind(event_type)
    .bind(host)
    .bind(description)
    .bind(status)
    .bind(published)
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
    sqlx::query_as::<_, EventPositionRow>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST, id ASC LIMIT $2 OFFSET $3"
    )
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
    sqlx::query_as::<_, EventPositionRow>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST"
    )
    .bind(event_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_published_event_positions(
    pool: &PgPool,
    cid: i64,
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
        "#,
    )
    .bind(cid)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_event_position(
    pool: &PgPool,
    id: &str,
    event_id: &str,
    callsign: &str,
    user_id: &str,
    requested_slot: Option<i32>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<EventPosition, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(
        "INSERT INTO events.event_positions (id, event_id, callsign, user_id, requested_slot, status, published, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at"
    )
    .bind(id)
    .bind(event_id)
    .bind(callsign)
    .bind(user_id)
    .bind(requested_slot)
    .bind("REQUESTED")
    .bind(false)
    .bind(now)
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
    sqlx::query_as::<_, EventPositionRow>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE id = $1 AND event_id = $2"
    )
    .bind(position_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn assign_event_position_row(
    pool: &PgPool,
    position_id: &str,
    event_id: &str,
    assigned_slot: i32,
    user_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<EventPosition>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(
        "UPDATE events.event_positions
         SET assigned_slot = $1, status = 'ASSIGNED', user_id = $2, updated_at = $3
         WHERE id = $4 AND event_id = $5
         RETURNING id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at"
    )
    .bind(assigned_slot)
    .bind(user_id)
    .bind(now)
    .bind(position_id)
    .bind(event_id)
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
        "select id, title, positions_locked, manual_positions_open, featured_fields, preset_positions, featured_field_configs, tmis, ops_free_text, ops_plan_published, ops_planner_id, enable_buffer_times, updated_at from events.events where id = $1",
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
        returning id, title, positions_locked, manual_positions_open, featured_fields, preset_positions, featured_field_configs, tmis, ops_free_text, ops_plan_published, ops_planner_id, enable_buffer_times, updated_at
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
        "select id, event_id, tmi_type, start_time, notes, created_at, updated_at from events.event_tmis where event_id = $1 order by start_time asc, created_at asc, id asc limit $2 offset $3",
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
    start_time: chrono::DateTime<chrono::Utc>,
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
