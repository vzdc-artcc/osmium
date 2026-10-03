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

/// Resource type for server-level impersonation audit rows. These are visible only
/// to SERVER_ADMIN readers — facility admins holding `audit.logs.read` must never
/// see impersonation activity (security checklist #2).
pub const AUTH_IMPERSONATION_RESOURCE: &str = "AUTH_IMPERSONATION";
/// Keys redacted from audit snapshots, matched exactly (case-insensitive). Exact
/// matching rather than substring matching keeps ordinary domain fields such as
/// `callsign`, `assigned_slot`, `session_id`, `category_key` and `state` readable.
/// `sig` is the signed-download signature and `secret_hash` the stored API key
/// digest; both carry credential material under a name the suffix rule misses.
const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "session_token",
    "secret",
    "client_secret",
    "password",
    "api_key",
    "apikey",
    "signature",
    "code_verifier",
    "sig",
    "secret_hash",
];

/// Any key ending with one of these is redacted too, so new credential columns
/// (`bearer_token`, `oauth_client_secret`, ...) are covered without a list edit.
const SENSITIVE_KEY_SUFFIXES: &[&str] = &["_secret", "_token", "_password"];

#[derive(Debug, Clone)]
pub struct AuditLogFilters {
    pub resource_type: Option<String>,
    /// Multi-value resource_type filter (domain views pass a set). ANDed with the
    /// singular `resource_type` above; domain pages set only this one.
    pub resource_types: Option<Vec<String>>,
    pub resource_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_type: Option<String>,
    pub scope_type: Option<String>,
    pub scope_key: Option<String>,
    pub action: Option<String>,
    pub limit: i64,
    pub offset: i64,
    /// When false, `AUTH_IMPERSONATION` rows are excluded (non-SERVER_ADMIN readers).
    pub include_server_sensitive: bool,
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
    /// Human-readable summary of what happened, composed at write time and shown
    /// as the "Message" column on the website. Optional so legacy/un-updated call
    /// sites are simply blank rather than failing.
    pub message: Option<String>,
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
            l.message,
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
          and ($10::bool or l.resource_type <> 'AUTH_IMPERSONATION')
          and ($11::text[] is null or l.resource_type = any($11))
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
    .bind(filters.include_server_sensitive)
    .bind(filters.resource_types.as_deref())
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Count companion for [`fetch_audit_logs`] — same filter predicates, for the
/// paginated response's total. `limit`/`offset` on the passed filters are ignored.
/// Moved verbatim out of `handlers/admin.rs::list_audit_logs` per spec 009.
pub async fn count_audit_logs(pool: &PgPool, filters: &AuditLogFilters) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from access.audit_logs l
        left join access.actors a on a.id = l.actor_id
        where ($1::text is null or l.resource_type = $1)
          and ($2::text is null or l.resource_id = $2)
          and ($3::text is null or l.actor_id = $3)
          and ($4::text is null or a.actor_type = $4)
          and ($5::text is null or l.scope_type = $5)
          and ($6::text is null or l.scope_key = $6)
          and ($7::text is null or l.action = $7)
          and ($8::bool or l.resource_type <> 'AUTH_IMPERSONATION')
          and ($9::text[] is null or l.resource_type = any($9))
        "#,
    )
    .bind(filters.resource_type.as_deref())
    .bind(filters.resource_id.as_deref())
    .bind(filters.actor_id.as_deref())
    .bind(filters.actor_type.as_deref())
    .bind(filters.scope_type.as_deref())
    .bind(filters.scope_key.as_deref())
    .bind(filters.action.as_deref())
    .bind(filters.include_server_sensitive)
    .bind(filters.resource_types.as_deref())
    .fetch_one(pool)
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
        // While impersonating, durable audit entries are attributed to the real
        // admin (the impersonator), never the impersonated victim — security
        // checklist #1. `audit_actor_user_id()` resolves to the impersonator when
        // present, otherwise the acting user.
        return Ok(AuditActor {
            actor_id: lookup_user_actor_id(executor, user.audit_actor_user_id()).await?,
        });
    }

    if let Some(service_account) = current_service_account {
        return Ok(AuditActor {
            actor_id: lookup_service_account_actor_id(executor, &service_account.id).await?,
        });
    }

    Ok(AuditActor { actor_id: None })
}

pub async fn record_audit<'e, E>(executor: E, mut entry: AuditEntryInput) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    // Fall back to a readable auto-composed summary when a call site doesn't pass
    // a hand-written message, so every audit row has a non-empty "Message".
    let message = entry.message.take().or_else(|| {
        Some(default_audit_message(
            &entry.action,
            &entry.resource_type,
            entry.resource_id.as_deref(),
        ))
    });
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
            message,
            before_state,
            after_state,
            ip_address,
            created_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::inet, now())
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(entry.actor_id)
    .bind(entry.action)
    .bind(entry.resource_type)
    .bind(entry.resource_id)
    .bind(entry.scope_type)
    .bind(entry.scope_key)
    .bind(message)
    .bind(entry.before_state)
    .bind(entry.after_state)
    .bind(entry.ip_address)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

/// Composes a readable default message like "Created training session (abc123)"
/// from an entry's action / resource_type / resource_id. Used by the per-handler
/// audit helpers so every log has a human-readable "Message" without threading a
/// hand-written string through ~180 call sites; inline sites may pass a richer one.
pub fn default_audit_message(
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
) -> String {
    let verb = match action.to_ascii_uppercase().as_str() {
        "CREATE" => "Created",
        "UPDATE" => "Updated",
        "DELETE" => "Deleted",
        "PUBLISH" => "Published",
        "ASSIGN" => "Assigned",
        "UNASSIGN" => "Unassigned",
        "REVOKE" => "Revoked",
        "UPSERT" => "Saved",
        "RUN" => "Ran",
        "DECIDE" => "Decided",
        "EXPORT" | "EXPORT_ALL" => "Exported",
        // Unknown action: title-case it ("APPROVE" -> "Approve", "REVOKE_ALL" ->
        // "Revoke all") so it reads like the known verbs, not raw uppercase.
        other => {
            let verb = humanize_action(other);
            let noun = humanize_resource(resource_type);
            return match resource_id {
                Some(id) => format!("{verb} {noun} ({id})"),
                None => format!("{verb} {noun}"),
            };
        }
    };
    let noun = humanize_resource(resource_type);
    match resource_id {
        Some(id) => format!("{verb} {noun} ({id})"),
        None => format!("{verb} {noun}"),
    }
}

fn humanize_resource(resource_type: &str) -> String {
    resource_type.to_ascii_lowercase().replace('_', " ")
}

/// Lower-cases an action, swaps `_` for spaces, and capitalizes the first letter,
/// so an un-mapped action renders as a Title-case verb ("Approve", "Revoke all").
fn humanize_action(action: &str) -> String {
    let lower = action.to_ascii_lowercase().replace('_', " ");
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => lower,
    }
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

/// Re-exported for existing call sites (`audit::client_ip(...)`). The
/// implementation now lives in [`crate::auth::ip`] as the single source of truth
/// shared with `logging.rs` and the rate limiter (spec 010).
pub use crate::auth::ip::client_ip;

/// Resolves the `access.actors` id for a user by their `user_id`, if provisioned.
/// Public so the data-export assembler can attribute a *subject's own* activity
/// log by uid (as opposed to `resolve_audit_actor`, which is impersonator-aware
/// and keyed off the acting `CurrentUser`).
pub async fn lookup_user_actor_id<'e, E>(
    executor: E,
    user_id: &str,
) -> Result<Option<String>, ApiError>
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
    SENSITIVE_KEYS.contains(&normalized.as_str())
        || SENSITIVE_KEY_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
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

    #[test]
    fn keeps_domain_fields_that_resemble_sensitive_names() {
        let value = json!({
            "callsign": "DEN_APP",
            "pilot_callsign": "AAL123",
            "assigned_slot": 2,
            "session_id": "sess-1",
            "category_key": "sops",
            "state": "CO",
            "nested": [{ "callsign": "DEN_TWR", "session_id": "sess-2" }]
        });

        assert_eq!(sanitize_value(value.clone()), value);
    }

    #[test]
    fn redacts_every_sensitive_name_top_level_nested_and_in_arrays() {
        let names = [
            "authorization",
            "Cookie",
            "token",
            "access_token",
            "refresh_token",
            "id_token",
            "session_token",
            "secret",
            "client_secret",
            "password",
            "api_key",
            "apikey",
            "signature",
            "code_verifier",
            "sig",
            "secret_hash",
            "bearer_token",
            "oauth_client_secret",
            "smtp_password",
        ];
        for name in names {
            let value = json!({
                name: "s3cr3t",
                "nested": { name: "s3cr3t", "safe": "ok" },
                "list": [{ name: "s3cr3t" }]
            });
            let sanitized = sanitize_value(value);
            assert_eq!(sanitized[name], "[REDACTED]", "top-level {name}");
            assert_eq!(sanitized["nested"][name], "[REDACTED]", "nested {name}");
            assert_eq!(sanitized["nested"]["safe"], "ok");
            assert_eq!(sanitized["list"][0][name], "[REDACTED]", "array {name}");
        }
    }
}
