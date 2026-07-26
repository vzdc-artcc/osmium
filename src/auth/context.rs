use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CurrentUser {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub timezone: String,
    pub rating: Option<String>,
    pub primary_role: Option<String>,
    // Impersonation (spec 012). When this session is an impersonation session,
    // `id`/`cid` above are the *target* (so ACL and UI resolve as them) and these
    // carry the real admin. `#[sqlx(default)]` so the many CurrentUser queries that
    // don't select them (e.g. `find_current_user_by_cid`) still build cleanly — only
    // the session-token lookup populates them.
    #[sqlx(default)]
    pub impersonator_user_id: Option<String>,
    #[sqlx(default)]
    pub impersonator_cid: Option<i64>,
    #[sqlx(default)]
    pub impersonator_display_name: Option<String>,
}

impl CurrentUser {
    /// True when this session is acting as another user via impersonation.
    pub fn is_impersonated(&self) -> bool {
        self.impersonator_user_id.is_some()
    }

    /// The user id to attribute durable audit entries to: the real admin when
    /// impersonating, otherwise the acting user themselves. Never the victim
    /// (security checklist #1).
    pub fn audit_actor_user_id(&self) -> &str {
        self.impersonator_user_id.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CurrentServiceAccount {
    pub id: String,
    pub key: String,
    pub name: String,
}

/// Newtype wrapper for the session cookie value so it can live in request
/// extensions without colliding with the bearer token (both are `Option<String>`,
/// and axum extensions are keyed by type — a bare `Option<String>` for each would
/// silently overwrite the other).
#[derive(Debug, Clone)]
pub struct SessionToken(pub Option<String>);
