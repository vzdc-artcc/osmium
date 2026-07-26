//! Session persistence for the auth handlers (spec 009 extraction).
//!
//! These are issued at the pool level, after `bootstrap_login_user`'s own
//! transaction has already committed — the atomicity boundary is unchanged from
//! the prior inline `handlers/auth.rs` calls, per spec 009's explicit note not to
//! silently alter it.

use sqlx::PgPool;

use crate::errors::ApiError;

/// Inserts a new 30-day login session. SQL moved verbatim out of
/// `handlers/auth.rs` (`vatsim_callback`). Callers keep their own `tracing`
/// context by wrapping the returned error.
pub async fn insert_session(
    pool: &PgPool,
    session_token: &str,
    user_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into identity.sessions (session_token, user_id, expires_at)
        values ($1, $2, now() + interval '30 days')
        "#,
    )
    .bind(session_token)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

/// Revokes a session by token. SQL moved verbatim out of
/// `handlers/auth.rs::logout`.
pub async fn delete_session(pool: &PgPool, session_token: &str) -> Result<(), ApiError> {
    sqlx::query("delete from identity.sessions where session_token = $1")
        .bind(session_token)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}
