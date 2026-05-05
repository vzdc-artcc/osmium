use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::ApiError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeTokenClaims {
    pub category: String,
    pub email: String,
    pub user_id: Option<String>,
}

pub fn build_unsubscribe_link(
    base_url: &str,
    secret: &str,
    category: &str,
    email: &str,
    user_id: Option<&str>,
) -> Option<String> {
    let token = sign_unsubscribe_token(
        secret,
        &UnsubscribeTokenClaims {
            category: category.to_string(),
            email: email.to_string(),
            user_id: user_id.map(str::to_string),
        },
    )
    .ok()?;

    Some(format!(
        "{}/api/v1/emails/unsubscribe?token={}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(&token)
    ))
}

pub fn sign_unsubscribe_token(
    secret: &str,
    claims: &UnsubscribeTokenClaims,
) -> Result<String, ApiError> {
    let payload = serde_json::to_vec(claims).map_err(|_| ApiError::Internal)?;
    let payload_encoded = URL_SAFE_NO_PAD.encode(payload);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::Internal)?;
    mac.update(payload_encoded.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(format!("{payload_encoded}.{sig}"))
}

pub fn verify_unsubscribe_token(
    secret: &str,
    token: &str,
) -> Result<UnsubscribeTokenClaims, ApiError> {
    let mut parts = token.split('.');
    let payload = parts.next().ok_or(ApiError::BadRequest)?;
    let sig = parts.next().ok_or(ApiError::BadRequest)?;
    if parts.next().is_some() {
        return Err(ApiError::BadRequest);
    }

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::Internal)?;
    mac.update(payload.as_bytes());
    mac.verify_slice(
        &URL_SAFE_NO_PAD
            .decode(sig)
            .map_err(|_| ApiError::BadRequest)?,
    )
    .map_err(|_| ApiError::BadRequest)?;

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| ApiError::BadRequest)?;
    serde_json::from_slice(&payload_bytes).map_err(|_| ApiError::BadRequest)
}

pub async fn is_suppressed(pool: &PgPool, category: &str, email: &str) -> Result<bool, ApiError> {
    let suppressed = sqlx::query_scalar::<_, bool>(
        r#"
        select exists(
            select 1
            from email.suppressions
            where category_id = $1
              and lower(email::text) = lower($2)
              and revoked_at is null
        )
        "#,
    )
    .bind(category)
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(suppressed)
}

pub async fn create_suppression(
    pool: &PgPool,
    claims: &UnsubscribeTokenClaims,
    source: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into email.suppressions (id, category_id, user_id, email, reason, source)
        select $1, $2, $3, $4, 'unsubscribed', $5
        where not exists (
            select 1
            from email.suppressions
            where category_id = $2
              and lower(email::text) = lower($4)
              and revoked_at is null
        )
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&claims.category)
    .bind(&claims.user_id)
    .bind(&claims.email)
    .bind(source)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn revoke_suppression(
    pool: &PgPool,
    category: &str,
    email: &str,
) -> Result<bool, ApiError> {
    let rows = sqlx::query(
        r#"
        update email.suppressions
        set revoked_at = now()
        where category_id = $1
          and lower(email::text) = lower($2)
          and revoked_at is null
        "#,
    )
    .bind(category)
    .bind(email)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .rows_affected();

    Ok(rows > 0)
}

#[cfg(test)]
mod tests {
    use super::{UnsubscribeTokenClaims, sign_unsubscribe_token, verify_unsubscribe_token};

    #[test]
    fn token_round_trip_is_stable() {
        let claims = UnsubscribeTokenClaims {
            category: "announcements".to_string(),
            email: "user@example.com".to_string(),
            user_id: Some("user-1".to_string()),
        };
        let token = sign_unsubscribe_token("secret", &claims).unwrap();
        let decoded = verify_unsubscribe_token("secret", &token).unwrap();
        assert_eq!(decoded.category, claims.category);
        assert_eq!(decoded.email, claims.email);
        assert_eq!(decoded.user_id, claims.user_id);
    }
}
