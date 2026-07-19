use serde_json::Value;
use sqlx::{Executor, PgPool, Postgres};

use crate::{
    errors::ApiError,
    models::integrations::{
        DiscordCategoryItem, DiscordChannelItem, DiscordConfigItem, DiscordRoleItem,
        OutboundJobItem,
    },
};

#[derive(Debug, sqlx::FromRow)]
pub struct DiscordOauthStateRow {
    pub external_id: String,
    pub metadata: Value,
}

pub async fn find_discord_link_by_user(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select external_id from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and local_id = $1",
    ).bind(user_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn upsert_discord_oauth_state(
    pool: &PgPool,
    id: &str,
    state_token: &str,
    user_id: &str,
    metadata: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into integration.external_sync_mappings (id, system_code, entity_type, local_id, external_id, metadata, created_at, updated_at)
        values ($1, 'discord', 'oauth_state', $2, $3, $4, now(), now())
        on conflict (system_code, entity_type, local_id) do update
        set external_id = excluded.external_id,
            metadata = excluded.metadata,
            updated_at = now()
        "#,
    )
    .bind(id)
    .bind(state_token)
    .bind(user_id)
    .bind(metadata)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn delete_discord_link(pool: &PgPool, user_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and local_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_discord_oauth_state(
    pool: &PgPool,
    state_token: &str,
) -> Result<Option<DiscordOauthStateRow>, ApiError> {
    sqlx::query_as::<_, DiscordOauthStateRow>(
        "select external_id, metadata from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'oauth_state' and local_id = $1",
    )
    .bind(state_token)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_discord_identity_owner<'e, E>(
    executor: E,
    external_id: &str,
) -> Result<Option<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>(
        "select local_id from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and external_id = $1",
    )
    .bind(external_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_discord_user_identity<'e, E>(
    executor: E,
    id: &str,
    user_id: &str,
    external_id: &str,
    metadata: Value,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into integration.external_sync_mappings (id, system_code, entity_type, local_id, external_id, metadata, created_at, updated_at)
        values ($1, 'discord', 'user_identity', $2, $3, $4, now(), now())
        on conflict (system_code, entity_type, local_id) do update
        set external_id = excluded.external_id,
            metadata = excluded.metadata,
            updated_at = now()
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(external_id)
    .bind(metadata)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn delete_discord_oauth_state<'e, E>(
    executor: E,
    state_token: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "delete from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'oauth_state' and local_id = $1",
    )
    .bind(state_token)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn list_discord_configs(pool: &PgPool) -> Result<Vec<DiscordConfigItem>, ApiError> {
    sqlx::query_as::<_, DiscordConfigItem>(
        "select id, name, guild_id, created_at, updated_at from integration.discord_configs order by name asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn list_discord_channels(pool: &PgPool) -> Result<Vec<DiscordChannelItem>, ApiError> {
    sqlx::query_as::<_, DiscordChannelItem>(
        "select id, discord_config_id, name, channel_id, created_at from integration.discord_channels order by name asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn list_discord_roles(pool: &PgPool) -> Result<Vec<DiscordRoleItem>, ApiError> {
    sqlx::query_as::<_, DiscordRoleItem>(
        "select id, discord_config_id, name, role_id, created_at from integration.discord_roles order by name asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn list_discord_categories(pool: &PgPool) -> Result<Vec<DiscordCategoryItem>, ApiError> {
    sqlx::query_as::<_, DiscordCategoryItem>(
        "select id, discord_config_id, name, category_id, created_at from integration.discord_categories order by name asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn insert_discord_config(
    pool: &PgPool,
    id: &str,
    name: &str,
    guild_id: Option<&str>,
) -> Result<DiscordConfigItem, ApiError> {
    sqlx::query_as::<_, DiscordConfigItem>(
        "insert into integration.discord_configs (id, name, guild_id, created_at, updated_at) values ($1, $2, $3, now(), now()) returning id, name, guild_id, created_at, updated_at",
    ).bind(id).bind(name).bind(guild_id).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn update_discord_config_row(
    pool: &PgPool,
    config_id: &str,
    name: Option<&str>,
    guild_id_set: bool,
    guild_id: Option<String>,
) -> Result<Option<DiscordConfigItem>, ApiError> {
    sqlx::query_as::<_, DiscordConfigItem>(
        r#"
        update integration.discord_configs
        set name = coalesce($2, name),
            guild_id = case when $3::bool then $4 else guild_id end,
            updated_at = now()
        where id = $1
        returning id, name, guild_id, created_at, updated_at
        "#,
    )
    .bind(config_id)
    .bind(name)
    .bind(guild_id_set)
    .bind(guild_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_discord_channel(
    pool: &PgPool,
    id: &str,
    discord_config_id: &str,
    name: &str,
    channel_id: &str,
) -> Result<DiscordChannelItem, ApiError> {
    sqlx::query_as::<_, DiscordChannelItem>(
        "insert into integration.discord_channels (id, discord_config_id, name, channel_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, channel_id, created_at",
    ).bind(id).bind(discord_config_id).bind(name).bind(channel_id).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn update_discord_channel_row(
    pool: &PgPool,
    channel_id: &str,
    name: Option<&str>,
    new_channel_id: Option<&str>,
) -> Result<Option<DiscordChannelItem>, ApiError> {
    sqlx::query_as::<_, DiscordChannelItem>(
        "update integration.discord_channels set name = coalesce($2, name), channel_id = coalesce($3, channel_id) where id = $1 returning id, discord_config_id, name, channel_id, created_at",
    ).bind(channel_id).bind(name).bind(new_channel_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_discord_channel_row(pool: &PgPool, channel_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from integration.discord_channels where id = $1")
        .bind(channel_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn insert_discord_role(
    pool: &PgPool,
    id: &str,
    discord_config_id: &str,
    name: &str,
    role_id: &str,
) -> Result<DiscordRoleItem, ApiError> {
    sqlx::query_as::<_, DiscordRoleItem>(
        "insert into integration.discord_roles (id, discord_config_id, name, role_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, role_id, created_at",
    ).bind(id).bind(discord_config_id).bind(name).bind(role_id).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn update_discord_role_row(
    pool: &PgPool,
    role_id: &str,
    name: Option<&str>,
    new_role_id: Option<&str>,
) -> Result<Option<DiscordRoleItem>, ApiError> {
    sqlx::query_as::<_, DiscordRoleItem>(
        "update integration.discord_roles set name = coalesce($2, name), role_id = coalesce($3, role_id) where id = $1 returning id, discord_config_id, name, role_id, created_at",
    ).bind(role_id).bind(name).bind(new_role_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_discord_role_row(pool: &PgPool, role_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from integration.discord_roles where id = $1")
        .bind(role_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn insert_discord_category(
    pool: &PgPool,
    id: &str,
    discord_config_id: &str,
    name: &str,
    category_id: &str,
) -> Result<DiscordCategoryItem, ApiError> {
    sqlx::query_as::<_, DiscordCategoryItem>(
        "insert into integration.discord_categories (id, discord_config_id, name, category_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, category_id, created_at",
    ).bind(id).bind(discord_config_id).bind(name).bind(category_id).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)
}

pub async fn update_discord_category_row(
    pool: &PgPool,
    category_id: &str,
    name: Option<&str>,
    new_category_id: Option<&str>,
) -> Result<Option<DiscordCategoryItem>, ApiError> {
    sqlx::query_as::<_, DiscordCategoryItem>(
        "update integration.discord_categories set name = coalesce($2, name), category_id = coalesce($3, category_id) where id = $1 returning id, discord_config_id, name, category_id, created_at",
    ).bind(category_id).bind(name).bind(new_category_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn delete_discord_category_row(pool: &PgPool, category_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from integration.discord_categories where id = $1")
        .bind(category_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn enqueue_outbound_job(
    pool: &PgPool,
    job_type: &str,
    subject_type: Option<&str>,
    subject_id: Option<&str>,
    payload: Value,
) -> Result<OutboundJobItem, ApiError> {
    sqlx::query_as::<_, OutboundJobItem>(
        r#"
        insert into integration.outbound_jobs (
            id, job_type, subject_type, subject_id, status, attempt_count, payload, created_at, updated_at
        )
        values ($1, $2, $3, $4, 'pending', 0, $5, now(), now())
        returning id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        "#,
    ).bind(uuid::Uuid::new_v4().to_string()).bind(job_type).bind(subject_type).bind(subject_id).bind(payload).fetch_one(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn count_outbound_jobs(pool: &PgPool, status: Option<&str>) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from integration.outbound_jobs where ($1::text is null or status = $1)",
    )
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_outbound_jobs(
    pool: &PgPool,
    status: Option<&str>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<OutboundJobItem>, ApiError> {
    sqlx::query_as::<_, OutboundJobItem>(
        r#"
        select id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        from integration.outbound_jobs
        where ($1::text is null or status = $1)
        order by created_at desc, id asc
        limit $2 offset $3
        "#,
    ).bind(status).bind(page_size).bind(offset).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn list_pending_outbound_jobs(pool: &PgPool) -> Result<Vec<OutboundJobItem>, ApiError> {
    sqlx::query_as::<_, OutboundJobItem>(
        r#"
        select id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        from integration.outbound_jobs
        where status in ('pending', 'retry')
          and (next_attempt_at is null or next_attempt_at <= now())
        order by created_at asc
        limit 20
        "#,
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)
}

pub async fn update_outbound_job_result(
    pool: &PgPool,
    id: &str,
    status: &str,
    attempt_count: i32,
    next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    error: Option<String>,
) -> Result<OutboundJobItem, ApiError> {
    sqlx::query_as::<_, OutboundJobItem>(
        r#"
        update integration.outbound_jobs
        set status = $2,
            attempt_count = $3,
            last_attempt_at = now(),
            next_attempt_at = $4,
            error = $5,
            updated_at = now()
        where id = $1
        returning id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(attempt_count)
    .bind(next_attempt_at)
    .bind(error)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
