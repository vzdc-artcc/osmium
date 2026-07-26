use sqlx::PgPool;

/// Liveness probe for the database connection. Returns `true` when a trivial
/// `select 1` succeeds, `false` on any error — matching the previous inline
/// `.is_ok()` behavior in `handlers/health.rs::ready` (errors are swallowed, not
/// surfaced, because `/ready` reports "degraded" rather than failing the request).
///
/// Extracted per spec 009 so "no inline SQL in handlers" stays a total, greppable
/// invariant even for a one-line query.
pub async fn is_database_ready(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(pool)
        .await
        .is_ok()
}
