use axum::http::HeaderMap;
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::{Executor, PgPool, Postgres};
use uuid::Uuid;

use crate::{
    auth::context::{CurrentServiceAccount, CurrentUser},
    errors::ApiError,
    models::AuditLogItem,
};

const REDACTED: &str = "[REDACTED]";
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "authorization",
    "cookie",
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "key",
    "session",
    "sig",
    "signature",
    "code",
    "state",
];

#[derive(Debug, Clone)]
pub struct AuditLogFilters {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_type: Option<String>,
    pub scope_type: Option<String>,
    pub scope_key: Option<String>,
    pub action: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct AuditActor {
    pub actor_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditEntryInput {
    pub actor_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub scope_type: String,
    pub scope_key: Option<String>,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub ip_address: Option<String>,
}

pub async fn fetch_audit_logs(
    pool: &PgPool,
    filters: &AuditLogFilters,
) -> Result<Vec<AuditLogItem>, ApiError> {
    sqlx::query_as::<_, AuditLogItem>(
        r#"
        select
            l.id,
            l.actor_id,
            a.actor_type,
            a.display_name as actor_display_name,
            a.user_id as actor_user_id,
            a.service_account_id as actor_service_account_id,
            l.action,
            l.resource_type,
            l.resource_id,
            l.scope_type,
            l.scope_key,
            l.before_state,
            l.after_state,
            l.ip_address::text as ip_address,
            l.created_at
        from access.audit_logs l
        left join access.actors a on a.id = l.actor_id
        where ($1::text is null or l.resource_type = $1)
          and ($2::text is null or l.resource_id = $2)
          and ($3::text is null or l.actor_id = $3)
          and ($4::text is null or a.actor_type = $4)
          and ($5::text is null or l.scope_type = $5)
          and ($6::text is null or l.scope_key = $6)
          and ($7::text is null or l.action = $7)
        order by l.created_at desc
        limit $8 offset $9
        "#,
    )
    .bind(filters.resource_type.as_deref())
    .bind(filters.resource_id.as_deref())
    .bind(filters.actor_id.as_deref())
    .bind(filters.actor_type.as_deref())
    .bind(filters.scope_type.as_deref())
    .bind(filters.scope_key.as_deref())
    .bind(filters.action.as_deref())
    .bind(filters.limit)
    .bind(filters.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn resolve_audit_actor<'e, E>(
    executor: E,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
) -> Result<AuditActor, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    if let Some(user) = current_user {
        return Ok(AuditActor {
            actor_id: lookup_user_actor_id(executor, &user.id).await?,
        });
    }

    if let Some(service_account) = current_service_account {
        return Ok(AuditActor {
            actor_id: lookup_service_account_actor_id(executor, &service_account.id).await?,
        });
    }

    Ok(AuditActor { actor_id: None })
}

pub async fn record_audit<'e, E>(executor: E, entry: AuditEntryInput) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into access.audit_logs (
            id,
            actor_id,
            action,
            resource_type,
            resource_id,
            scope_type,
            scope_key,
            before_state,
            after_state,
            ip_address,
            created_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::inet, now())
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(entry.actor_id)
    .bind(entry.action)
    .bind(entry.resource_type)
    .bind(entry.resource_id)
    .bind(entry.scope_type)
    .bind(entry.scope_key)
    .bind(entry.before_state)
    .bind(entry.after_state)
    .bind(entry.ip_address)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub fn sanitize_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut sanitized = Map::with_capacity(object.len());
            for (key, value) in object {
                if is_sensitive_key(&key) {
                    sanitized.insert(key, Value::String(REDACTED.to_string()));
                } else {
                    sanitized.insert(key, sanitize_value(value));
                }
            }
            Value::Object(sanitized)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize_value).collect()),
        other => other,
    }
}

pub fn sanitized_snapshot<T: Serialize>(value: &T) -> Result<Value, ApiError> {
    let raw = serde_json::to_value(value).map_err(|_| ApiError::Internal)?;
    Ok(sanitize_value(raw))
}

pub fn client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(first) = value.split(',').next() {
            let parsed = first.trim();
            if !parsed.is_empty() {
                if parsed.parse::<std::net::IpAddr>().is_ok() {
                    return Some(parsed.to_string());
                }
            }
        }
    }

    if let Some(value) = headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
    {
        let parsed = value.trim();
        if !parsed.is_empty() {
            if parsed.parse::<std::net::IpAddr>().is_ok() {
                return Some(parsed.to_string());
            }
        }
    }

    None
}

async fn lookup_user_actor_id<'e, E>(executor: E, user_id: &str) -> Result<Option<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>(
        "select id from access.actors where actor_type = 'user' and user_id = $1 limit 1",
    )
    .bind(user_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn lookup_service_account_actor_id<'e, E>(
    executor: E,
    service_account_id: &str,
) -> Result<Option<String>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>(
        "select id from access.actors where actor_type = 'service_account' and service_account_id = $1 limit 1",
    )
    .bind(service_account_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    SENSITIVE_KEY_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::sanitize_value;

    #[test]
    fn redacts_sensitive_fields_recursively() {
        let value = json!({
            "token": "abc",
            "nested": {
                "api_key": "def",
                "safe": "ok"
            }
        });

        let sanitized = sanitize_value(value);
        assert_eq!(sanitized["token"], "[REDACTED]");
        assert_eq!(sanitized["nested"]["api_key"], "[REDACTED]");
        assert_eq!(sanitized["nested"]["safe"], "ok");
    }
}
