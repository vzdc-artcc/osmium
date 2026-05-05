use serde_json::Value;
use sqlx::PgPool;

use crate::{errors::ApiError, models::EmailAudienceRequest};

#[derive(Debug, Clone)]
pub struct ResolvedRecipient {
    pub user_id: Option<String>,
    pub email: String,
    pub display_name: Option<String>,
    pub source: String,
}

#[derive(sqlx::FromRow)]
struct AudienceRow {
    id: String,
    email: String,
    display_name: String,
}

pub async fn resolve_audience(
    pool: &PgPool,
    audience: &EmailAudienceRequest,
) -> Result<Vec<ResolvedRecipient>, ApiError> {
    let roles = if audience.roles.is_empty() {
        None
    } else {
        Some(audience.roles.as_slice())
    };
    let artcc = if audience.artcc.is_empty() {
        None
    } else {
        Some(audience.artcc.as_slice())
    };
    let rating = if audience.rating.is_empty() {
        None
    } else {
        Some(audience.rating.as_slice())
    };

    let rows = sqlx::query_as::<_, AudienceRow>(
        r#"
        select distinct
            u.id,
            coalesce(u.email::text, '') as email,
            u.display_name
        from identity.users u
        left join org.memberships m on m.user_id = u.id
        left join identity.user_profiles p on p.user_id = u.id
        left join access.user_roles ur on ur.user_id = u.id
        where coalesce(u.email::text, '') <> ''
          and ($1::text[] is null or ur.role_name = any($1))
          and ($2::text[] is null or m.artcc = any($2))
          and ($3::text[] is null or m.rating = any($3))
          and ($4::boolean is null or coalesce(p.new_event_notifications, false) = $4)
          and ($5::boolean is null or coalesce(u.status, '') = 'active')
        order by u.display_name asc
        "#,
    )
    .bind(roles)
    .bind(artcc)
    .bind(rating)
    .bind(audience.receive_event_notifications)
    .bind(audience.active_only)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            normalize_email(&row.email).map(|email| ResolvedRecipient {
                user_id: Some(row.id),
                email,
                display_name: Some(row.display_name),
                source: "audience".to_string(),
            })
        })
        .collect())
}

pub fn normalize_email(email: &str) -> Option<String> {
    let normalized = email.trim().to_ascii_lowercase();
    if normalized.is_empty() || !normalized.contains('@') {
        return None;
    }

    let mut parts = normalized.split('@');
    let local = parts.next()?;
    let domain = parts.next()?;
    if local.is_empty() || domain.is_empty() || parts.next().is_some() || !domain.contains('.') {
        return None;
    }

    Some(normalized)
}

pub fn audience_to_value(audience: Option<&EmailAudienceRequest>) -> Option<Value> {
    audience.and_then(|value| serde_json::to_value(value).ok())
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn normalizes_email_case() {
        assert_eq!(
            normalize_email(" USER@Example.COM ").as_deref(),
            Some("user@example.com")
        );
    }
}
