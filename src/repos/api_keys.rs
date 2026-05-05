use sqlx::{PgPool, Postgres, Transaction};

use crate::errors::ApiError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub prefix: Option<String>,
    pub last_four: Option<String>,
    pub created_by_user_id: Option<String>,
    pub created_by_display_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewApiKeyInput<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub created_by_user_id: &'a str,
    pub credential_id: &'a str,
    pub secret_hash: &'a str,
    pub prefix: &'a str,
    pub last_four: &'a str,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub permissions: &'a [String],
}

pub async fn insert_api_key(
    tx: &mut Transaction<'_, Postgres>,
    input: NewApiKeyInput<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into access.service_accounts (
            id, key, name, description, status, kind, created_by_user_id
        )
        values ($1, $2, $3, $4, 'active', 'api_key', $5)
        "#,
    )
    .bind(input.id)
    .bind(input.key)
    .bind(input.name)
    .bind(input.description)
    .bind(input.created_by_user_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into access.service_account_credentials (
            id,
            service_account_id,
            credential_type,
            secret_hash,
            prefix,
            last_four,
            expires_at
        )
        values ($1, $2, 'api_key', $3, $4, $5, $6)
        "#,
    )
    .bind(input.credential_id)
    .bind(input.id)
    .bind(input.secret_hash)
    .bind(input.prefix)
    .bind(input.last_four)
    .bind(input.expires_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into access.actors (id, actor_type, service_account_id, display_name)
        values (gen_random_uuid()::text, 'service_account', $1, $2)
        on conflict do nothing
        "#,
    )
    .bind(input.id)
    .bind(input.name)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    replace_service_account_permissions(tx, input.id, input.permissions).await?;

    Ok(())
}

pub async fn replace_service_account_permissions(
    tx: &mut Transaction<'_, Postgres>,
    service_account_id: &str,
    permissions: &[String],
) -> Result<(), ApiError> {
    sqlx::query("delete from access.service_account_permissions where service_account_id = $1")
        .bind(service_account_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    for permission_name in permissions {
        sqlx::query(
            r#"
            insert into access.service_account_permissions (
                service_account_id,
                permission_name,
                granted
            )
            values ($1, $2, true)
            on conflict (service_account_id, permission_name) do update
            set granted = true
            "#,
        )
        .bind(service_account_id)
        .bind(permission_name)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

const API_KEY_SELECT_BODY: &str = r#"
    select
        sa.id,
        sa.key,
        sa.name,
        sa.description,
        sa.status,
        cred.prefix,
        cred.last_four,
        sa.created_by_user_id,
        u.display_name as created_by_display_name,
        sa.created_at,
        cred.last_used_at,
        cred.expires_at,
        cred.revoked_at
    from access.service_accounts sa
    left join identity.users u on u.id = sa.created_by_user_id
    left join lateral (
        select prefix, last_four, last_used_at, expires_at, revoked_at
        from access.service_account_credentials sac
        where sac.service_account_id = sa.id
        order by sac.created_at desc
        limit 1
    ) cred on true
    where sa.kind = 'api_key'
"#;

pub async fn list_api_keys(
    pool: &PgPool,
    viewer_user_id: &str,
    viewer_can_read_all: bool,
) -> Result<Vec<ApiKeyRow>, ApiError> {
    let query = format!(
        "{API_KEY_SELECT_BODY}\n      and ($1::bool or sa.created_by_user_id = $2)\n   order by sa.created_at desc"
    );

    sqlx::query_as::<_, ApiKeyRow>(&query)
        .bind(viewer_can_read_all)
        .bind(viewer_user_id)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn fetch_api_key(pool: &PgPool, key_id: &str) -> Result<Option<ApiKeyRow>, ApiError> {
    let query = format!("{API_KEY_SELECT_BODY}\n      and sa.id = $1");

    sqlx::query_as::<_, ApiKeyRow>(&query)
        .bind(key_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn fetch_api_key_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    key_id: &str,
) -> Result<Option<ApiKeyRow>, ApiError> {
    let query = format!("{API_KEY_SELECT_BODY}\n      and sa.id = $1");

    sqlx::query_as::<_, ApiKeyRow>(&query)
        .bind(key_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn update_api_key_metadata(
    tx: &mut Transaction<'_, Postgres>,
    key_id: &str,
    name: Option<&str>,
    description_update: DescriptionUpdate<'_>,
) -> Result<(), ApiError> {
    let (set_description, description_value) = match description_update {
        DescriptionUpdate::Unchanged => (false, None),
        DescriptionUpdate::Set(value) => (true, value),
    };

    sqlx::query(
        r#"
        update access.service_accounts
        set
            name = coalesce($1, name),
            description = case when $2::bool then $3 else description end
        where id = $4 and kind = 'api_key'
        "#,
    )
    .bind(name)
    .bind(set_description)
    .bind(description_value)
    .bind(key_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum DescriptionUpdate<'a> {
    Unchanged,
    Set(Option<&'a str>),
}

pub async fn revoke_api_key(
    tx: &mut Transaction<'_, Postgres>,
    key_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update access.service_account_credentials
        set revoked_at = coalesce(revoked_at, now())
        where service_account_id = $1
        "#,
    )
    .bind(key_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        update access.service_accounts
        set status = 'disabled'
        where id = $1 and kind = 'api_key'
        "#,
    )
    .bind(key_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_api_key_permission_names(
    pool: &PgPool,
    service_account_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select permission_name
        from access.service_account_permissions
        where service_account_id = $1
          and granted is true
        order by permission_name
        "#,
    )
    .bind(service_account_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_api_key_permission_names_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    service_account_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select permission_name
        from access.service_account_permissions
        where service_account_id = $1
          and granted is true
        order by permission_name
        "#,
    )
    .bind(service_account_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}
